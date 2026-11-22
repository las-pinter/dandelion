//! Errors returned by every terminal backend.

use thiserror::Error;

/// Anything a backend can fail with.
///
/// Backend errors are kept as domain values so the web layer can map them to
/// HTTP status codes without matching on strings.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum TerminalError {
    /// The backend could not be reached, or its socket went away.
    #[error("terminal backend unavailable: {0}")]
    Unavailable(String),

    /// The backend answered an operation it does not implement.
    #[error("unsupported terminal operation: {0}")]
    Unsupported(String),

    /// The backend was detected but its version or protocol is not known.
    #[error("terminal backend is read-only: {0}")]
    ReadOnly(String),

    /// The wire protocol was violated or an answer could not be understood.
    #[error("terminal protocol error: {0}")]
    Protocol(String),

    /// The terminal does not exist (any more).
    #[error("terminal not found: {0}")]
    NotFound(String),

    /// The program is waiting on a person and refused the input.
    #[error("terminal is blocked: {0}")]
    Blocked(String),

    /// The program was in another state than the caller asked for.
    #[error("terminal status mismatch: {0}")]
    StatusMismatch(String),

    /// The terminal is still live where a finished one is required.
    #[error("terminal is still live: {0}")]
    StillLive(String),

    /// The key name is not acceptable to the backends.
    #[error("invalid key name: {0}")]
    InvalidKey(String),

    /// The operation timed out.
    #[error("terminal operation timed out")]
    Timeout,

    /// The stream or session was already closed.
    #[error("terminal stream is closed")]
    Closed,

    /// A backend domain error without a dedicated variant.
    #[error("backend error: {message}")]
    Backend {
        /// JSON-RPC error code, when the backend used one.
        code: Option<i64>,
        /// Stable backend error kind such as `not_found`.
        kind: Option<String>,
        /// Human-readable message.
        message: String,
        /// Backend hint for the caller.
        hint: Option<String>,
    },

    /// Local I/O failed.
    #[error("terminal I/O failed: {0}")]
    Io(#[from] std::io::Error),

    /// JSON encoding or decoding failed.
    #[error("terminal JSON failed: {0}")]
    Json(#[from] serde_json::Error),
}

impl TerminalError {
    /// Builds a backend domain error.
    #[must_use]
    pub fn backend(
        code: Option<i64>,
        kind: Option<String>,
        message: impl Into<String>,
        hint: Option<String>,
    ) -> Self {
        Self::Backend {
            code,
            kind,
            message: message.into(),
            hint,
        }
    }

    /// Returns true when the terminal no longer exists.
    #[must_use]
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound(_))
    }
}

#[cfg(test)]
mod tests {
    use super::TerminalError;

    #[test]
    fn not_found_predicate_matches_only_not_found() {
        assert!(TerminalError::NotFound("t9".into()).is_not_found());
        assert!(!TerminalError::Blocked("t9".into()).is_not_found());
    }

    #[test]
    fn backend_error_keeps_kind_and_hint() {
        let error = TerminalError::backend(
            Some(-32000),
            Some("blocked".into()),
            "t3 is blocked",
            Some("answer it first".into()),
        );

        let shown = error.to_string();

        assert!(shown.contains("t3 is blocked"), "{shown}");
        assert!(!shown.contains("answer it first"), "{shown}");
    }
}
