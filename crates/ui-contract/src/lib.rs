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
    Show {
        query: Option<String>,
        mode: UiMode,
    },
    Hide,
    Toggle {
        query: Option<String>,
        mode: UiMode,
    },
    ApplySnapshot(UiSnapshot),
    Shutdown,
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
    fn snapshot(&self) -> UiSnapshot;
}

/// Headless backend used when the binary is built without a GUI implementation.
///
/// It still accepts logical snapshots and shutdown/hide commands, but refuses
/// commands that require a visible surface. This prevents a headless build from
/// silently pretending that `show` or `toggle` succeeded.
#[derive(Debug, Default)]
pub struct UnavailableUiBackend {
    snapshot: UiSnapshot,
}

impl UiBackend for UnavailableUiBackend {
    fn dispatch(&mut self, command: UiCommand) -> Result<UiReceipt, UiError> {
        match command {
            UiCommand::ApplySnapshot(snapshot) => {
                self.snapshot = snapshot;
                Ok(UiReceipt::Accepted)
            }
            UiCommand::Hide => {
                self.snapshot.visibility = UiVisibility::Hidden;
                self.snapshot.generation = self.snapshot.generation.saturating_add(1);
                Ok(UiReceipt::Accepted)
            }
            UiCommand::Shutdown => Ok(UiReceipt::Accepted),
            UiCommand::Show { .. } | UiCommand::Toggle { .. } => Err(UiError::UnavailableInBuild),
        }
    }

    fn snapshot(&self) -> UiSnapshot {
        self.snapshot.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        UiBackend, UiCommand, UiError, UiMode, UiSnapshot, UiVisibility, UnavailableUiBackend,
    };

    #[test]
    fn headless_backend_rejects_visible_surface_commands() {
        let mut backend = UnavailableUiBackend::default();

        let result = backend.dispatch(UiCommand::Show {
            query: None,
            mode: UiMode::Launcher,
        });

        assert!(matches!(result, Err(UiError::UnavailableInBuild)));
    }

    #[test]
    fn headless_backend_accepts_logical_snapshots() {
        let mut backend = UnavailableUiBackend::default();
        let snapshot = UiSnapshot {
            visibility: UiVisibility::Visible,
            mode: UiMode::Agent,
            query: Some("status".to_owned()),
            generation: 7,
        };

        let result = backend.dispatch(UiCommand::ApplySnapshot(snapshot.clone()));

        assert!(result.is_ok());
        assert_eq!(backend.snapshot(), snapshot);
    }
}
