use std::{
    fs, io,
    os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};

use loncher_applications::ApplicationEntry;
use loncher_domain::{
    DAEMON_PROTOCOL_VERSION, DaemonCommand, DaemonReply, DaemonState, LauncherMode, ProtocolError,
    ProtocolErrorCode, ReplyEnvelope, RequestEnvelope, RequestId, UiVisibility,
};
use loncher_search::{SearchRequest, SearchService};
use loncher_ui_contract::{
    UiBackend, UiCommand, UiError, UiMode, UiSnapshot, UiVisibility as ContractVisibility,
};
use thiserror::Error;
use tokio::{
    net::{UnixListener, UnixStream},
    sync::{mpsc, oneshot},
    task::JoinSet,
    time::timeout,
};
use tokio_util::sync::CancellationToken;
use tracing::{Instrument, debug, error, info, warn};

use crate::{config::RuntimeConfig, transport};

const STALE_SOCKET_PROBE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(250);

pub async fn run_daemon_with_ui<U>(
    config: RuntimeConfig,
    cancellation: CancellationToken,
    ui: U,
) -> Result<(), ServerError>
where
    U: UiBackend + 'static,
{
    run_daemon_with_ui_and_applications(config, cancellation, ui, Vec::new()).await
}

pub async fn run_daemon_with_ui_and_applications<U>(
    config: RuntimeConfig,
    cancellation: CancellationToken,
    ui: U,
    applications: Vec<ApplicationEntry>,
) -> Result<(), ServerError>
where
    U: UiBackend + 'static,
{
    let bound = bind_listener(&config).await?;
    let expected_uid = bound.owner_uid;
    let listener = bound.listener;
    let _cleanup = SocketCleanup::new(config.socket_path.clone());
    let (request_tx, request_rx) = mpsc::channel(config.command_queue_capacity);

    info!(socket = %config.socket_path.display(), "daemon IPC listener ready");

    let listener_cancellation = cancellation.child_token();
    let listener_config = config.clone();
    let listener_task = tokio::spawn(async move {
        run_listener(listener, listener_config, expected_uid, request_tx, listener_cancellation)
            .await
    });

    let router_result = run_router(ui, request_rx, cancellation.clone(), applications).await;
    cancellation.cancel();

    let listener_result = listener_task.await.map_err(ServerError::ListenerTask)?;
    listener_result?;
    router_result?;

    info!(socket = %config.socket_path.display(), "daemon IPC listener stopped");
    Ok(())
}

struct BoundListener {
    listener: UnixListener,
    owner_uid: u32,
}

async fn bind_listener(config: &RuntimeConfig) -> Result<BoundListener, ServerError> {
    let parent = config.socket_parent()?;
    prepare_parent_directory(parent)?;
    recover_stale_socket(&config.socket_path).await?;

    let listener = match UnixListener::bind(&config.socket_path) {
        Ok(listener) => listener,
        Err(error) if error.kind() == io::ErrorKind::AddrInUse => {
            return Err(ServerError::InstanceAlreadyRunning);
        }
        Err(error) => return Err(ServerError::Io(error)),
    };

    fs::set_permissions(&config.socket_path, fs::Permissions::from_mode(0o600))?;
    let owner_uid = fs::metadata(&config.socket_path)?.uid();
    let parent_uid = fs::metadata(parent)?.uid();
    if parent_uid != owner_uid {
        drop(listener);
        fs::remove_file(&config.socket_path)?;
        return Err(ServerError::SocketDirectoryOwnerMismatch {
            path: parent.to_path_buf(),
            expected_uid: owner_uid,
            actual_uid: parent_uid,
        });
    }

    Ok(BoundListener { listener, owner_uid })
}

fn prepare_parent_directory(parent: &Path) -> Result<(), ServerError> {
    match fs::symlink_metadata(parent) {
        Ok(metadata) => validate_existing_parent(parent, &metadata),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let mut builder = fs::DirBuilder::new();
            builder.mode(0o700);
            match builder.create(parent) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    let metadata = fs::symlink_metadata(parent)?;
                    validate_existing_parent(parent, &metadata)
                }
                Err(error) => Err(ServerError::Io(error)),
            }
        }
        Err(error) => Err(ServerError::Io(error)),
    }
}

fn validate_existing_parent(parent: &Path, metadata: &fs::Metadata) -> Result<(), ServerError> {
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(ServerError::UnsafeSocketPath(parent.to_path_buf()));
    }

    let mode = metadata.permissions().mode() & 0o777;
    if mode != 0o700 {
        return Err(ServerError::InsecureSocketDirectory { path: parent.to_path_buf(), mode });
    }

    Ok(())
}

async fn recover_stale_socket(path: &Path) -> Result<(), ServerError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(ServerError::Io(error)),
    };

    if metadata.file_type().is_symlink() || !metadata.file_type().is_socket() {
        return Err(ServerError::UnsafeSocketPath(path.to_path_buf()));
    }

    match timeout(STALE_SOCKET_PROBE_TIMEOUT, UnixStream::connect(path)).await {
        Ok(Ok(_stream)) => Err(ServerError::InstanceAlreadyRunning),
        Ok(Err(error))
            if matches!(
                error.kind(),
                io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound
            ) =>
        {
            fs::remove_file(path)?;
            Ok(())
        }
        Ok(Err(error)) => Err(ServerError::SocketProbe(error)),
        Err(_) => Err(ServerError::InstanceAlreadyRunning),
    }
}

async fn run_listener(
    listener: UnixListener,
    config: RuntimeConfig,
    expected_uid: u32,
    request_tx: mpsc::Sender<RoutedRequest>,
    cancellation: CancellationToken,
) -> Result<(), ServerError> {
    let mut connections = JoinSet::new();
    let connection_limit = config.command_queue_capacity;

    loop {
        tokio::select! {
            _ = cancellation.cancelled() => break,
            accepted = listener.accept(), if connections.len() < connection_limit => {
                let (stream, _address) = accepted?;
                let peer_uid = stream.peer_cred()?.uid();
                if peer_uid != expected_uid {
                    warn!(peer_uid, expected_uid, "rejected IPC client owned by another user");
                    continue;
                }

                let connection_tx = request_tx.clone();
                let connection_cancellation = cancellation.child_token();
                let max_frame_size = config.max_frame_size;
                let request_timeout = config.request_timeout;
                connections.spawn(async move {
                    timeout(
                        request_timeout,
                        handle_connection(
                            stream,
                            peer_uid,
                            max_frame_size,
                            connection_tx,
                            connection_cancellation,
                        ),
                    )
                    .await
                    .map_err(|_| ConnectionError::Timeout(request_timeout))?
                });
            }
            joined = connections.join_next(), if !connections.is_empty() => {
                inspect_connection_result(joined)?;
            }
        }
    }

    drop(request_tx);
    while let Some(joined) = connections.join_next().await {
        inspect_connection_result(Some(joined))?;
    }

    Ok(())
}

fn inspect_connection_result(
    joined: Option<Result<Result<(), ConnectionError>, tokio::task::JoinError>>,
) -> Result<(), ServerError> {
    match joined {
        Some(Ok(Ok(()))) | None => Ok(()),
        Some(Ok(Err(error))) => {
            debug!(%error, "IPC connection ended with an error");
            Ok(())
        }
        Some(Err(error)) => Err(ServerError::ConnectionTask(error)),
    }
}

async fn handle_connection(
    stream: UnixStream,
    peer_uid: u32,
    max_frame_size: usize,
    request_tx: mpsc::Sender<RoutedRequest>,
    cancellation: CancellationToken,
) -> Result<(), ConnectionError> {
    let mut framed = transport::framed(stream, max_frame_size);
    let received = tokio::select! {
        _ = cancellation.cancelled() => return Ok(()),
        received = transport::receive_json::<RequestEnvelope>(&mut framed) => received,
    };
    let envelope = match received {
        Ok(envelope) => envelope,
        Err(error) => {
            let reply = ReplyEnvelope::error(
                RequestId::UNKNOWN,
                ProtocolError::new(ProtocolErrorCode::InvalidFrame, "invalid request frame"),
            );
            let _send_result = transport::send_json(&mut framed, &reply).await;
            return Err(ConnectionError::Transport(error));
        }
    };

    let request_id = envelope.request_id;
    let command_kind = envelope.command.kind();
    let span = tracing::info_span!(
        "daemon_request",
        request_id = request_id.get(),
        command_kind,
        peer_uid,
    );

    async move {
        if envelope.protocol_version != DAEMON_PROTOCOL_VERSION {
            let reply = ReplyEnvelope::error(
                request_id,
                ProtocolError::new(
                    ProtocolErrorCode::UnsupportedVersion,
                    format!(
                        "unsupported protocol version {}; expected {}",
                        envelope.protocol_version, DAEMON_PROTOCOL_VERSION
                    ),
                ),
            );
            transport::send_json(&mut framed, &reply).await?;
            return Ok(());
        }

        let (reply_tx, reply_rx) = oneshot::channel();
        tokio::select! {
            _ = cancellation.cancelled() => return Ok(()),
            sent = request_tx.send(RoutedRequest { envelope, respond_to: reply_tx }) => {
                sent.map_err(|_| ConnectionError::RouterUnavailable)?;
            }
        }

        let reply = tokio::select! {
            biased;
            reply = reply_rx => reply.map_err(|_| ConnectionError::RouterUnavailable)?,
            _ = cancellation.cancelled() => return Ok(()),
        };
        transport::send_json(&mut framed, &reply).await?;
        Ok(())
    }
    .instrument(span)
    .await
}

struct RoutedRequest {
    envelope: RequestEnvelope,
    respond_to: oneshot::Sender<ReplyEnvelope>,
}

async fn run_router<U>(
    mut ui: U,
    mut requests: mpsc::Receiver<RoutedRequest>,
    cancellation: CancellationToken,
    applications: Vec<ApplicationEntry>,
) -> Result<(), ServerError>
where
    U: UiBackend,
{
    let mut state = DaemonState::default();
    let search = SearchService::new(applications, 12);
    ui.dispatch(UiCommand::ApplySnapshot(to_ui_snapshot(&state.snapshot(), &search)))?;

    loop {
        tokio::select! {
            _ = cancellation.cancelled() => break,
            request = requests.recv() => {
                let Some(request) = request else { break };
                let request_id = request.envelope.request_id;
                let command = request.envelope.command;
                let shutdown = matches!(command, DaemonCommand::Shutdown);
                let reply = route_command(&mut state, &mut ui, request_id, &command, &search);

                if request.respond_to.send(reply).is_err() {
                    debug!(request_id = request_id.get(), "IPC client dropped before reply");
                }

                if shutdown {
                    cancellation.cancel();
                    break;
                }
            }
        }
    }

    ui.dispatch(UiCommand::ApplySnapshot(UiSnapshot::default()))?;
    ui.dispatch(UiCommand::Shutdown)?;
    Ok(())
}

fn route_command<U>(
    state: &mut DaemonState,
    ui: &mut U,
    request_id: RequestId,
    command: &DaemonCommand,
    search: &SearchService,
) -> ReplyEnvelope
where
    U: UiBackend,
{
    if let DaemonCommand::Status = command {
        return ReplyEnvelope::success(
            request_id,
            DaemonReply::Status { snapshot: state.snapshot() },
        );
    }

    if let DaemonCommand::Shutdown = command {
        return ReplyEnvelope::success(
            request_id,
            DaemonReply::Accepted { snapshot: state.snapshot() },
        );
    }

    let next = match state.reduce(command) {
        Ok(next) => next,
        Err(error) => {
            return ReplyEnvelope::error(
                request_id,
                ProtocolError::new(ProtocolErrorCode::InvalidCommand, error.to_string()),
            );
        }
    };

    if next.snapshot() != state.snapshot() {
        if let Err(error) =
            ui.dispatch(UiCommand::ApplySnapshot(to_ui_snapshot(&next.snapshot(), search)))
        {
            let (code, message) = public_ui_error(error);
            return ReplyEnvelope::error(request_id, ProtocolError::new(code, message));
        }
        *state = next;
    }

    ReplyEnvelope::success(request_id, DaemonReply::Accepted { snapshot: state.snapshot() })
}

fn to_ui_snapshot(snapshot: &loncher_domain::DaemonSnapshot, search: &SearchService) -> UiSnapshot {
    let response = search
        .search(SearchRequest {
            generation: snapshot.generation,
            query: snapshot.query.clone().unwrap_or_default(),
        })
        .unwrap_or_else(|_| loncher_search::SearchResponse {
            generation: snapshot.generation,
            query: String::new(),
            results: Vec::new(),
        });
    UiSnapshot {
        visibility: match snapshot.visibility {
            UiVisibility::Hidden => ContractVisibility::Hidden,
            UiVisibility::Visible => ContractVisibility::Visible,
        },
        mode: match snapshot.mode {
            LauncherMode::Launcher => UiMode::Launcher,
            LauncherMode::Terminal => UiMode::Terminal,
            LauncherMode::Agent => UiMode::Agent,
        },
        query: snapshot.query.clone(),
        generation: snapshot.generation,
        results: response
            .results
            .into_iter()
            .map(|result| loncher_ui_contract::ApplicationViewModel {
                desktop_id: result.application.desktop_id,
                name: result.application.name,
                generic_name: result.application.generic_name,
                icon_path: result.application.icon.and_then(|icon| icon.resolved_path),
            })
            .collect(),
        selected: 0,
    }
}

fn public_ui_error(error: UiError) -> (ProtocolErrorCode, String) {
    match error {
        UiError::UnavailableInBuild => (
            ProtocolErrorCode::UiUnavailable,
            "GUI support is unavailable in this build".to_owned(),
        ),
        UiError::Rejected(reason) => {
            error!(reason, "UI backend rejected a daemon snapshot");
            (ProtocolErrorCode::Internal, "UI backend rejected the requested state".to_owned())
        }
    }
}

struct SocketCleanup {
    path: PathBuf,
}

impl SocketCleanup {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for SocketCleanup {
    fn drop(&mut self) {
        match fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                warn!(path = %self.path.display(), %error, "failed to remove daemon socket")
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum ServerError {
    #[error(transparent)]
    Config(#[from] crate::config::ConfigError),
    #[error("another loncher daemon is already running")]
    InstanceAlreadyRunning,
    #[error("unsafe socket path: {0}")]
    UnsafeSocketPath(PathBuf),
    #[error("socket directory must have mode 0700: {path} has mode {mode:o}")]
    InsecureSocketDirectory { path: PathBuf, mode: u32 },
    #[error(
        "socket directory owner mismatch for {path}: expected UID {expected_uid}, got {actual_uid}"
    )]
    SocketDirectoryOwnerMismatch { path: PathBuf, expected_uid: u32, actual_uid: u32 },
    #[error("socket probe failed: {0}")]
    SocketProbe(io::Error),
    #[error("daemon I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Ui(#[from] UiError),
    #[error("listener task failed: {0}")]
    ListenerTask(tokio::task::JoinError),
    #[error("connection task failed: {0}")]
    ConnectionTask(tokio::task::JoinError),
}

#[derive(Debug, Error)]
enum ConnectionError {
    #[error(transparent)]
    Transport(#[from] transport::TransportError),
    #[error("daemon command router is unavailable")]
    RouterUnavailable,
    #[error("daemon request timed out after {0:?}")]
    Timeout(std::time::Duration),
}
