#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const DAEMON_PROTOCOL_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RequestId(u64);

impl RequestId {
    pub const UNKNOWN: Self = Self(0);

    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiVisibility {
    #[default]
    Hidden,
    Visible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LauncherMode {
    #[default]
    Launcher,
    Terminal,
    Agent,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DaemonSnapshot {
    pub visibility: UiVisibility,
    pub mode: LauncherMode,
    pub query: Option<String>,
    pub generation: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DaemonState {
    snapshot: DaemonSnapshot,
}

impl DaemonState {
    pub fn snapshot(&self) -> DaemonSnapshot {
        self.snapshot.clone()
    }

    pub fn reduce(&self, command: &DaemonCommand) -> Result<Self, CommandValidationError> {
        command.validate()?;

        let mut next = self.clone();
        match command {
            DaemonCommand::Show { query } => {
                next.snapshot.visibility = UiVisibility::Visible;
                next.snapshot.mode = LauncherMode::Launcher;
                next.snapshot.query = query.clone();
            }
            DaemonCommand::Hide => {
                next.snapshot.visibility = UiVisibility::Hidden;
            }
            DaemonCommand::Toggle { query } => {
                if self.snapshot.visibility == UiVisibility::Visible {
                    next.snapshot.visibility = UiVisibility::Hidden;
                } else {
                    next.snapshot.visibility = UiVisibility::Visible;
                    next.snapshot.mode = LauncherMode::Launcher;
                    next.snapshot.query = query.clone();
                }
            }
            DaemonCommand::Query { text } => {
                next.snapshot.visibility = UiVisibility::Visible;
                next.snapshot.mode = LauncherMode::Launcher;
                next.snapshot.query = Some(text.clone());
            }
            DaemonCommand::OpenAgent { prompt } => {
                next.snapshot.visibility = UiVisibility::Visible;
                next.snapshot.mode = LauncherMode::Agent;
                next.snapshot.query = prompt.clone();
            }
            DaemonCommand::Status | DaemonCommand::Shutdown => {}
        }

        if next.observable_state() != self.observable_state() {
            next.snapshot.generation = self.snapshot.generation.saturating_add(1);
        }

        Ok(next)
    }

    fn observable_state(&self) -> (UiVisibility, LauncherMode, &Option<String>) {
        (self.snapshot.visibility, self.snapshot.mode, &self.snapshot.query)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum DaemonCommand {
    Show { query: Option<String> },
    Hide,
    Toggle { query: Option<String> },
    Query { text: String },
    OpenAgent { prompt: Option<String> },
    Status,
    Shutdown,
}

impl DaemonCommand {
    pub fn validate(&self) -> Result<(), CommandValidationError> {
        match self {
            Self::Query { text } if text.trim().is_empty() => {
                Err(CommandValidationError::EmptyQuery)
            }
            Self::Show { query: Some(query) } | Self::Toggle { query: Some(query) }
                if query.trim().is_empty() =>
            {
                Err(CommandValidationError::EmptyQuery)
            }
            Self::OpenAgent { prompt: Some(prompt) } if prompt.trim().is_empty() => {
                Err(CommandValidationError::EmptyPrompt)
            }
            _ => Ok(()),
        }
    }

    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Show { .. } => "show",
            Self::Hide => "hide",
            Self::Toggle { .. } => "toggle",
            Self::Query { .. } => "query",
            Self::OpenAgent { .. } => "open_agent",
            Self::Status => "status",
            Self::Shutdown => "shutdown",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum DaemonReply {
    Accepted { snapshot: DaemonSnapshot },
    Status { snapshot: DaemonSnapshot },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestEnvelope {
    pub protocol_version: u16,
    pub request_id: RequestId,
    pub command: DaemonCommand,
}

impl RequestEnvelope {
    pub fn new(request_id: RequestId, command: DaemonCommand) -> Self {
        Self { protocol_version: DAEMON_PROTOCOL_VERSION, request_id, command }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplyEnvelope {
    pub protocol_version: u16,
    pub request_id: RequestId,
    pub payload: ReplyPayload,
}

impl ReplyEnvelope {
    pub fn success(request_id: RequestId, reply: DaemonReply) -> Self {
        Self {
            protocol_version: DAEMON_PROTOCOL_VERSION,
            request_id,
            payload: ReplyPayload::Success { reply },
        }
    }

    pub fn error(request_id: RequestId, error: ProtocolError) -> Self {
        Self {
            protocol_version: DAEMON_PROTOCOL_VERSION,
            request_id,
            payload: ReplyPayload::Error { error },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum ReplyPayload {
    Success { reply: DaemonReply },
    Error { error: ProtocolError },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolErrorCode {
    UnsupportedVersion,
    InvalidFrame,
    InvalidCommand,
    RequestTooLarge,
    UiUnavailable,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolError {
    pub code: ProtocolErrorCode,
    pub message: String,
}

impl ProtocolError {
    pub fn new(code: ProtocolErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CommandValidationError {
    #[error("query must not be empty")]
    EmptyQuery,
    #[error("agent prompt must not be empty")]
    EmptyPrompt,
}

#[cfg(test)]
mod tests {
    use super::{
        CommandValidationError, DaemonCommand, DaemonState, LauncherMode, ReplyEnvelope,
        ReplyPayload, RequestId, UiVisibility,
    };

    #[test]
    fn rejects_empty_query() {
        let command = DaemonCommand::Query { text: "   ".to_owned() };

        assert_eq!(command.validate(), Err(CommandValidationError::EmptyQuery));
    }

    #[test]
    fn show_is_idempotent_and_generation_tracks_changes() {
        let state = DaemonState::default();
        let command = DaemonCommand::Show { query: Some("zed".to_owned()) };

        let visible = state.reduce(&command).expect("valid transition");
        let repeated = visible.reduce(&command).expect("valid transition");

        assert_eq!(visible.snapshot().visibility, UiVisibility::Visible);
        assert_eq!(visible.snapshot().generation, 1);
        assert_eq!(repeated.snapshot().generation, 1);
    }

    #[test]
    fn agent_transition_sets_mode() {
        let state = DaemonState::default();
        let next = state
            .reduce(&DaemonCommand::OpenAgent { prompt: Some("status".to_owned()) })
            .expect("valid transition");

        assert_eq!(next.snapshot().mode, LauncherMode::Agent);
        assert_eq!(next.snapshot().query.as_deref(), Some("status"));
    }

    #[test]
    fn reply_wire_shape_is_stable() {
        let reply = ReplyEnvelope::success(
            RequestId::new(7),
            super::DaemonReply::Status { snapshot: Default::default() },
        );
        let json = serde_json::to_value(reply).expect("serializable reply");

        assert_eq!(json["protocol_version"], 1);
        assert_eq!(json["request_id"], 7);
        assert_eq!(json["payload"]["status"], "success");
        assert!(matches!(
            serde_json::from_value::<ReplyEnvelope>(json).expect("deserializable reply").payload,
            ReplyPayload::Success { .. }
        ));
    }
}
