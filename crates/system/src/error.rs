//! The crate-wide error type.

use crate::{docker::DockerError, process::ProcessError, systemd::SystemdError};

/// Any error raised by this crate.
#[derive(Debug, thiserror::Error)]
pub enum SystemError {
    /// A process operation failed.
    #[error(transparent)]
    Process(#[from] ProcessError),
    /// A systemd operation failed.
    #[error(transparent)]
    Systemd(#[from] SystemdError),
    /// A Docker operation failed.
    #[error(transparent)]
    Docker(#[from] DockerError),
}
