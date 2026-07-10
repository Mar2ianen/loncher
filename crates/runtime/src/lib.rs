#![forbid(unsafe_code)]

use loncher_domain::{CommandValidationError, DaemonCommand};
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error(transparent)]
    InvalidCommand(#[from] CommandValidationError),
}

pub async fn run_daemon(cancellation: CancellationToken) -> Result<(), RuntimeError> {
    info!("daemon runtime initialized");
    cancellation.cancelled().await;
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
