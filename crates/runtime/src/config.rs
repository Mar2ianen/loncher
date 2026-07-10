use std::{
    env,
    path::{Path, PathBuf},
    time::Duration,
};

use thiserror::Error;

const DEFAULT_REQUEST_TIMEOUT_MS: u64 = 2_000;
const DEFAULT_MAX_FRAME_SIZE: usize = 1024 * 1024;
const DEFAULT_COMMAND_QUEUE_CAPACITY: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    pub socket_path: PathBuf,
    pub request_timeout: Duration,
    pub max_frame_size: usize,
    pub command_queue_capacity: usize,
}

impl RuntimeConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        let socket_path = match env::var_os("LONCHER_SOCKET") {
            Some(path) if !path.is_empty() => PathBuf::from(path),
            _ => {
                let runtime_dir = env::var_os("XDG_RUNTIME_DIR")
                    .filter(|path| !path.is_empty())
                    .ok_or(ConfigError::RuntimeDirUnavailable)?;
                PathBuf::from(runtime_dir).join("loncher/loncher.sock")
            }
        };

        Ok(Self::for_socket(socket_path))
    }

    pub fn for_socket(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
            request_timeout: Duration::from_millis(DEFAULT_REQUEST_TIMEOUT_MS),
            max_frame_size: DEFAULT_MAX_FRAME_SIZE,
            command_queue_capacity: DEFAULT_COMMAND_QUEUE_CAPACITY,
        }
    }

    pub fn socket_parent(&self) -> Result<&Path, ConfigError> {
        self.socket_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .ok_or_else(|| ConfigError::InvalidSocketPath(self.socket_path.clone()))
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("XDG_RUNTIME_DIR is unavailable; set it or provide LONCHER_SOCKET explicitly")]
    RuntimeDirUnavailable,
    #[error("socket path has no parent directory: {0}")]
    InvalidSocketPath(PathBuf),
}
