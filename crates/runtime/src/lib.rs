#![forbid(unsafe_code)]

mod config;
mod server;
mod transport;

use std::sync::atomic::{AtomicU64, Ordering};

pub use config::{ConfigError, RuntimeConfig};
pub use server::{ServerError, run_daemon_with_ui, run_daemon_with_ui_and_applications};

use loncher_applications::discover;
use loncher_domain::{
    DaemonCommand, DaemonReply, ProtocolErrorCode, ReplyPayload, RequestEnvelope, RequestId,
};
use loncher_ui_contract::UnavailableUiBackend;
use thiserror::Error;
use tokio::{net::UnixStream, time::timeout};
use tokio_util::sync::CancellationToken;

static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

pub async fn run_daemon(cancellation: CancellationToken) -> Result<(), RuntimeError> {
    let config = RuntimeConfig::from_env()?;
    let report = tokio::task::spawn_blocking(discover)
        .await
        .map_err(|error| RuntimeError::Discovery(error.to_string()))?;
    run_daemon_with_ui_and_applications(
        config,
        cancellation,
        UnavailableUiBackend,
        report.applications,
    )
    .await?;
    Ok(())
}

pub async fn run_daemon_with_config(
    config: RuntimeConfig,
    cancellation: CancellationToken,
) -> Result<(), RuntimeError> {
    run_daemon_with_ui_and_applications(config, cancellation, UnavailableUiBackend, Vec::new())
        .await?;
    Ok(())
}

#[cfg(feature = "gui")]
pub async fn run_daemon_gui(cancellation: CancellationToken) -> Result<(), RuntimeError> {
    let config = RuntimeConfig::from_env()?;
    let report = tokio::task::spawn_blocking(discover)
        .await
        .map_err(|error| RuntimeError::Discovery(error.to_string()))?;
    let channels = loncher_ui_iced::channels();
    let backend = channels.backend.clone();
    let gui_task = tokio::task::spawn_blocking(move || loncher_ui_iced::run(channels));
    let daemon_result =
        run_daemon_with_ui_and_applications(config, cancellation, backend, report.applications)
            .await;
    let gui_result = gui_task.await.map_err(|error| RuntimeError::GuiTask(error.to_string()))?;
    gui_result.map_err(|error| RuntimeError::Gui(error.to_string()))?;
    daemon_result?;
    Ok(())
}

pub async fn dispatch_command(command: DaemonCommand) -> Result<DaemonReply, ClientError> {
    let config = RuntimeConfig::from_env()?;
    dispatch_command_with_config(&config, command).await
}

pub async fn dispatch_command_with_config(
    config: &RuntimeConfig,
    command: DaemonCommand,
) -> Result<DaemonReply, ClientError> {
    command.validate().map_err(|error| ClientError::InvalidCommand(error.to_string()))?;

    let request_id = RequestId::new(NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed));
    let request = RequestEnvelope::new(request_id, command);

    timeout(config.request_timeout, async {
        let stream =
            UnixStream::connect(&config.socket_path).await.map_err(ClientError::Connect)?;
        let mut framed = transport::framed(stream, config.max_frame_size);
        transport::send_json(&mut framed, &request).await?;
        let reply = transport::receive_json::<loncher_domain::ReplyEnvelope>(&mut framed).await?;

        if reply.request_id != request_id {
            return Err(ClientError::RequestIdMismatch {
                expected: request_id,
                actual: reply.request_id,
            });
        }

        match reply.payload {
            ReplyPayload::Success { reply } => Ok(reply),
            ReplyPayload::Error { error } => {
                Err(ClientError::Remote { code: error.code, message: error.message })
            }
        }
    })
    .await
    .map_err(|_| ClientError::Timeout(config.request_timeout))?
}

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Server(#[from] ServerError),
    #[error("application discovery task failed: {0}")]
    Discovery(String),
    #[error("GUI task failed: {0}")]
    GuiTask(String),
    #[error("GUI failed: {0}")]
    Gui(String),
}

#[derive(Debug, Error)]
pub enum ClientError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("invalid command: {0}")]
    InvalidCommand(String),
    #[error("failed to connect to daemon: {0}")]
    Connect(std::io::Error),
    #[error(transparent)]
    Transport(#[from] transport::TransportError),
    #[error("daemon request timed out after {0:?}")]
    Timeout(std::time::Duration),
    #[error("daemon reply request ID mismatch: expected {expected:?}, got {actual:?}")]
    RequestIdMismatch { expected: RequestId, actual: RequestId },
    #[error("daemon rejected request ({code:?}): {message}")]
    Remote { code: ProtocolErrorCode, message: String },
}
