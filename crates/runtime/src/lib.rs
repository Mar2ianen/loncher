#![forbid(unsafe_code)]

use loncher_domain::{CommandValidationError, DaemonCommand};
use loncher_ui_contract::{UiBackend, UiCommand, UiError, UnavailableUiBackend};
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error(transparent)]
    InvalidCommand(#[from] CommandValidationError),
    #[error(transparent)]
    Ui(#[from] UiError),
}

pub async fn run_daemon(cancellation: CancellationToken) -> Result<(), RuntimeError> {
    run_daemon_with_ui(cancellation, UnavailableUiBackend::default()).await
}

pub async fn run_daemon_with_ui<U>(
    cancellation: CancellationToken,
    mut ui: U,
) -> Result<(), RuntimeError>
where
    U: UiBackend,
{
    debug!(snapshot = ?ui.snapshot(), "UI backend attached");
    info!("daemon runtime initialized");

    cancellation.cancelled().await;

    ui.dispatch(UiCommand::Hide)?;
    ui.dispatch(UiCommand::Shutdown)?;
    info!("daemon runtime cancellation observed");
    Ok(())
}

pub async fn dispatch_command(command: DaemonCommand) -> Result<(), RuntimeError> {
    command.validate()?;

    // Phase 0 will replace this stub with a Unix-socket client. Keeping command validation
    // here prevents the CLI contract from accepting invalid state before IPC exists.
    debug!(?command, "command accepted by bootstrap runtime");
    Ok(())
}
