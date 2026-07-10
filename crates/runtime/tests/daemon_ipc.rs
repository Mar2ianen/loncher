use std::{fs, os::unix::fs::PermissionsExt, path::Path, time::Duration};

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use loncher_domain::{
    DAEMON_PROTOCOL_VERSION, DaemonCommand, DaemonReply, ProtocolErrorCode, ReplyEnvelope,
    ReplyPayload, RequestEnvelope, RequestId, UiVisibility,
};
use loncher_runtime::{
    ClientError, RuntimeConfig, ServerError, dispatch_command_with_config, run_daemon_with_ui,
};
use loncher_ui_contract::{
    UiBackend, UiCommand, UiError, UiReceipt, UiSnapshot, UnavailableUiBackend,
};
use tempfile::TempDir;
use tokio::{
    net::UnixStream,
    task::{JoinHandle, JoinSet},
    time::{sleep, timeout},
};
use tokio_util::{codec::LengthDelimitedCodec, sync::CancellationToken};

#[derive(Debug, Default)]
struct AcceptingUi {
    snapshot: UiSnapshot,
}

impl UiBackend for AcceptingUi {
    fn dispatch(&mut self, command: UiCommand) -> Result<UiReceipt, UiError> {
        match command {
            UiCommand::ApplySnapshot(snapshot) => self.snapshot = snapshot,
            UiCommand::Shutdown => {}
        }
        Ok(UiReceipt::Accepted)
    }
}

struct TestDaemon {
    _temp: TempDir,
    config: RuntimeConfig,
    cancellation: CancellationToken,
    task: JoinHandle<Result<(), ServerError>>,
}

impl TestDaemon {
    async fn start_accepting() -> Self {
        Self::start_with_ui(AcceptingUi::default()).await
    }

    async fn start_headless() -> Self {
        Self::start_with_ui(UnavailableUiBackend).await
    }

    async fn start_with_ui<U>(ui: U) -> Self
    where
        U: UiBackend + 'static,
    {
        let temp = tempfile::tempdir().expect("temporary runtime directory");
        let config = RuntimeConfig::for_socket(temp.path().join("loncher/loncher.sock"));
        let cancellation = CancellationToken::new();
        let daemon_cancellation = cancellation.child_token();
        let daemon_config = config.clone();
        let task = tokio::spawn(async move {
            run_daemon_with_ui(daemon_config, daemon_cancellation, ui).await
        });

        wait_for_socket(&config.socket_path).await;

        Self { _temp: temp, config, cancellation, task }
    }

    async fn shutdown(self) {
        let reply = dispatch_command_with_config(&self.config, DaemonCommand::Shutdown)
            .await
            .expect("shutdown request succeeds");
        assert!(matches!(reply, DaemonReply::Accepted { .. }));

        timeout(Duration::from_secs(2), self.task)
            .await
            .expect("daemon stops before timeout")
            .expect("daemon task joins")
            .expect("daemon exits cleanly");
        assert!(!self.config.socket_path.exists());
    }
}

async fn wait_for_socket(path: &Path) {
    timeout(Duration::from_secs(2), async {
        loop {
            match UnixStream::connect(path).await {
                Ok(stream) => {
                    drop(stream);
                    return;
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                    ) =>
                {
                    sleep(Duration::from_millis(10)).await;
                }
                Err(error) => panic!("unexpected socket readiness error: {error}"),
            }
        }
    })
    .await
    .expect("daemon accepts socket connections");
}

#[tokio::test]
async fn show_and_status_round_trip() {
    let daemon = TestDaemon::start_accepting().await;

    let shown = dispatch_command_with_config(
        &daemon.config,
        DaemonCommand::Show { query: Some("zed".to_owned()) },
    )
    .await
    .expect("show request succeeds");

    let shown_snapshot = match shown {
        DaemonReply::Accepted { snapshot } => snapshot,
        DaemonReply::Status { .. } => panic!("unexpected status reply"),
    };
    assert_eq!(shown_snapshot.visibility, UiVisibility::Visible);
    assert_eq!(shown_snapshot.query.as_deref(), Some("zed"));
    assert_eq!(shown_snapshot.generation, 1);

    let status = dispatch_command_with_config(&daemon.config, DaemonCommand::Status)
        .await
        .expect("status request succeeds");
    let status_snapshot = match status {
        DaemonReply::Status { snapshot } => snapshot,
        DaemonReply::Accepted { .. } => panic!("unexpected accepted reply"),
    };
    assert_eq!(status_snapshot, shown_snapshot);

    daemon.shutdown().await;
}

#[tokio::test]
async fn headless_visible_command_returns_typed_error_without_mutation() {
    let daemon = TestDaemon::start_headless().await;

    let error = dispatch_command_with_config(
        &daemon.config,
        DaemonCommand::Show { query: Some("zed".to_owned()) },
    )
    .await
    .expect_err("headless show must fail");

    assert!(matches!(error, ClientError::Remote { code: ProtocolErrorCode::UiUnavailable, .. }));

    let status = dispatch_command_with_config(&daemon.config, DaemonCommand::Status)
        .await
        .expect("status request succeeds");
    let snapshot = match status {
        DaemonReply::Status { snapshot } => snapshot,
        DaemonReply::Accepted { .. } => panic!("unexpected accepted reply"),
    };
    assert_eq!(snapshot.visibility, UiVisibility::Hidden);
    assert_eq!(snapshot.generation, 0);

    daemon.shutdown().await;
}

#[tokio::test]
async fn concurrent_clients_are_serialized_through_bounded_router() {
    let daemon = TestDaemon::start_accepting().await;
    let mut clients = JoinSet::new();

    for index in 0..16 {
        let config = daemon.config.clone();
        clients.spawn(async move {
            dispatch_command_with_config(
                &config,
                DaemonCommand::Show { query: Some(format!("query-{index}")) },
            )
            .await
        });
    }

    while let Some(result) = clients.join_next().await {
        result.expect("client task joins").expect("concurrent request succeeds");
    }

    let status = dispatch_command_with_config(&daemon.config, DaemonCommand::Status)
        .await
        .expect("status request succeeds");
    let snapshot = match status {
        DaemonReply::Status { snapshot } => snapshot,
        DaemonReply::Accepted { .. } => panic!("unexpected accepted reply"),
    };
    assert_eq!(snapshot.visibility, UiVisibility::Visible);
    assert!(snapshot.generation >= 1);

    daemon.shutdown().await;
}

#[tokio::test]
async fn second_daemon_is_rejected() {
    let daemon = TestDaemon::start_accepting().await;

    let error =
        run_daemon_with_ui(daemon.config.clone(), CancellationToken::new(), AcceptingUi::default())
            .await
            .expect_err("second daemon must fail");

    assert!(matches!(error, ServerError::InstanceAlreadyRunning));
    daemon.shutdown().await;
}

#[tokio::test]
async fn stale_socket_is_recovered() {
    let temp = tempfile::tempdir().expect("temporary runtime directory");
    let socket_path = temp.path().join("loncher.sock");
    let stale =
        std::os::unix::net::UnixListener::bind(&socket_path).expect("create stale socket fixture");
    drop(stale);
    assert!(socket_path.exists());

    let config = RuntimeConfig::for_socket(&socket_path);
    let cancellation = CancellationToken::new();
    let daemon_cancellation = cancellation.child_token();
    let daemon_config = config.clone();
    let task = tokio::spawn(async move {
        run_daemon_with_ui(daemon_config, daemon_cancellation, AcceptingUi::default()).await
    });
    wait_for_socket(&socket_path).await;

    let status = dispatch_command_with_config(&config, DaemonCommand::Status)
        .await
        .expect("recovered daemon accepts requests");
    assert!(matches!(status, DaemonReply::Status { .. }));

    dispatch_command_with_config(&config, DaemonCommand::Shutdown)
        .await
        .expect("shutdown request succeeds");
    task.await.expect("daemon task joins").expect("daemon exits cleanly");
}

#[tokio::test]
async fn malformed_frame_returns_public_protocol_error() {
    let daemon = TestDaemon::start_accepting().await;
    let stream = UnixStream::connect(&daemon.config.socket_path).await.expect("connect raw client");
    let codec =
        LengthDelimitedCodec::builder().max_frame_length(daemon.config.max_frame_size).new_codec();
    let mut framed = tokio_util::codec::Framed::new(stream, codec);

    framed.send(Bytes::from_static(b"{")).await.expect("send malformed JSON frame");
    let frame = framed.next().await.expect("server sends reply").expect("reply frame is valid");
    let reply: ReplyEnvelope = serde_json::from_slice(&frame).expect("reply is JSON");

    assert_eq!(reply.request_id, RequestId::UNKNOWN);
    assert!(matches!(
        reply.payload,
        ReplyPayload::Error {
            error: loncher_domain::ProtocolError { code: ProtocolErrorCode::InvalidFrame, .. }
        }
    ));

    daemon.shutdown().await;
}

#[tokio::test]
async fn unsupported_protocol_version_returns_typed_error() {
    let daemon = TestDaemon::start_accepting().await;
    let stream = UnixStream::connect(&daemon.config.socket_path).await.expect("connect raw client");
    let codec =
        LengthDelimitedCodec::builder().max_frame_length(daemon.config.max_frame_size).new_codec();
    let mut framed = tokio_util::codec::Framed::new(stream, codec);
    let mut request = RequestEnvelope::new(RequestId::new(41), DaemonCommand::Status);
    request.protocol_version = DAEMON_PROTOCOL_VERSION + 1;

    framed
        .send(Bytes::from(serde_json::to_vec(&request).expect("serialize request")))
        .await
        .expect("send request");
    let frame = framed.next().await.expect("server sends reply").expect("reply frame is valid");
    let reply: ReplyEnvelope = serde_json::from_slice(&frame).expect("reply is JSON");

    assert!(matches!(
        reply.payload,
        ReplyPayload::Error {
            error: loncher_domain::ProtocolError {
                code: ProtocolErrorCode::UnsupportedVersion,
                ..
            }
        }
    ));

    daemon.shutdown().await;
}

#[tokio::test]
async fn socket_permissions_and_external_cleanup_are_enforced() {
    let daemon = TestDaemon::start_accepting().await;
    let parent = daemon.config.socket_path.parent().expect("socket has parent");

    assert_eq!(fs::metadata(parent).expect("parent metadata").permissions().mode() & 0o777, 0o700);
    assert_eq!(
        fs::metadata(&daemon.config.socket_path).expect("socket metadata").permissions().mode()
            & 0o777,
        0o600
    );

    daemon.cancellation.cancel();
    timeout(Duration::from_secs(2), daemon.task)
        .await
        .expect("daemon stops before timeout")
        .expect("daemon task joins")
        .expect("daemon exits cleanly");
    assert!(!daemon.config.socket_path.exists());
}
