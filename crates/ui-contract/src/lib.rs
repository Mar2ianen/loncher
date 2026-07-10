#![forbid(unsafe_code)]

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UiVisibility {
    #[default]
    Hidden,
    Visible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UiMode {
    #[default]
    Launcher,
    Terminal,
    Agent,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UiSnapshot {
    pub visibility: UiVisibility,
    pub mode: UiMode,
    pub query: Option<String>,
    pub generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiCommand {
    ApplySnapshot(UiSnapshot),
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiEvent {
    DismissRequested,
    QueryChanged(String),
    SubmitRequested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiReceipt {
    Accepted,
}

#[derive(Debug, Error)]
pub enum UiError {
    #[error("GUI support is unavailable in this build")]
    UnavailableInBuild,
    #[error("UI backend rejected the command: {0}")]
    Rejected(&'static str),
}

pub trait UiBackend: Send {
    fn dispatch(&mut self, command: UiCommand) -> Result<UiReceipt, UiError>;
}

/// Headless backend used when the binary is built without a GUI implementation.
///
/// It accepts hidden snapshots and shutdown, but refuses snapshots that require
/// a visible surface. The backend deliberately stores no authoritative state:
/// the daemon owns the snapshot and only projects it into a frontend.
#[derive(Debug, Default)]
pub struct UnavailableUiBackend;

impl UiBackend for UnavailableUiBackend {
    fn dispatch(&mut self, command: UiCommand) -> Result<UiReceipt, UiError> {
        match command {
            UiCommand::ApplySnapshot(snapshot)
                if snapshot.visibility == UiVisibility::Visible =>
            {
                Err(UiError::UnavailableInBuild)
            }
            UiCommand::ApplySnapshot(_) | UiCommand::Shutdown => Ok(UiReceipt::Accepted),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        UiBackend, UiCommand, UiError, UiMode, UiSnapshot, UiVisibility, UnavailableUiBackend,
    };

    #[test]
    fn headless_backend_rejects_visible_snapshot() {
        let mut backend = UnavailableUiBackend;

        let result = backend.dispatch(UiCommand::ApplySnapshot(UiSnapshot {
            visibility: UiVisibility::Visible,
            mode: UiMode::Launcher,
            query: None,
            generation: 1,
        }));

        assert!(matches!(result, Err(UiError::UnavailableInBuild)));
    }

    #[test]
    fn headless_backend_accepts_hidden_snapshot() {
        let mut backend = UnavailableUiBackend;

        let result = backend.dispatch(UiCommand::ApplySnapshot(UiSnapshot::default()));

        assert!(result.is_ok());
    }
}
