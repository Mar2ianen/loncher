#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DaemonCommand {
    Show { query: Option<String> },
    Hide,
    Toggle { query: Option<String> },
    Query { text: String },
    OpenAgent { prompt: Option<String> },
    Status,
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
    use super::{CommandValidationError, DaemonCommand};

    #[test]
    fn rejects_empty_query() {
        let command = DaemonCommand::Query { text: "   ".to_owned() };

        assert_eq!(command.validate(), Err(CommandValidationError::EmptyQuery));
    }

    #[test]
    fn accepts_missing_agent_prompt() {
        let command = DaemonCommand::OpenAgent { prompt: None };

        assert_eq!(command.validate(), Ok(()));
    }
}
