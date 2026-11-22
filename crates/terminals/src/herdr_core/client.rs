//! A small herdr-core JSON-RPC client.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::time::timeout;

use crate::error::TerminalError;
use crate::jsonrpc::{self, Incoming, Request};
use crate::util::CALL_TIMEOUT;

use super::wire::HelloResult;

/// A connection factory for the herdr-core socket.
#[derive(Debug, Clone)]
pub struct CoreClient {
    socket: Arc<PathBuf>,
    next_id: Arc<AtomicU64>,
}

impl CoreClient {
    /// Targets an explicit socket path.
    #[must_use]
    pub fn new(socket: impl Into<PathBuf>) -> Self {
        Self {
            socket: Arc::new(socket.into()),
            next_id: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Targets the socket selected by the environment.
    #[must_use]
    pub fn from_environment() -> Self {
        Self::new(default_socket())
    }

    /// Returns the socket path in use.
    #[must_use]
    pub fn socket(&self) -> &Path {
        &self.socket
    }

    /// Opens one connection.
    ///
    /// # Errors
    ///
    /// Returns [`TerminalError::Unavailable`] when the socket cannot be dialled.
    pub async fn connect(&self) -> Result<UnixStream, TerminalError> {
        UnixStream::connect(self.socket.as_path())
            .await
            .map_err(|error| {
                TerminalError::Unavailable(format!("{}: {error}", self.socket.display()))
            })
    }

    /// Performs one request on a fresh connection.
    ///
    /// # Errors
    ///
    /// Returns the backend's domain error, [`TerminalError::Timeout`] after 30
    /// seconds, or [`TerminalError::Protocol`] for an unintelligible answer.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, TerminalError> {
        self.call_with_timeout(method, params, CALL_TIMEOUT).await
    }

    /// Performs one request with an explicit deadline.
    ///
    /// Use a longer deadline for calls that ask the core to wait server-side,
    /// such as `terminal.send` with `wait: true`.
    ///
    /// # Errors
    ///
    /// Returns the backend's domain error, [`TerminalError::Timeout`] when the
    /// deadline passes, or [`TerminalError::Protocol`] for an unintelligible
    /// answer.
    pub async fn call_with_timeout(
        &self,
        method: &str,
        params: Value,
        deadline: Duration,
    ) -> Result<Value, TerminalError> {
        let stream = self.connect().await?;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = Request::new(id, method, params);
        self.request_on(stream, &request, deadline).await
    }

    /// Sends one request over an existing connection and reads one line.
    pub(crate) async fn request_on(
        &self,
        mut stream: UnixStream,
        request: &Request,
        deadline: Duration,
    ) -> Result<Value, TerminalError> {
        stream.write_all(request.encode_line().as_bytes()).await?;
        stream.flush().await?;
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        let read = timeout(deadline, reader.read_line(&mut line))
            .await
            .map_err(|_| TerminalError::Timeout)??;
        if read == 0 {
            return Err(TerminalError::Protocol(
                "core closed the connection without an answer".into(),
            ));
        }
        match jsonrpc::parse_line(&line)? {
            Incoming::Response(response) => response.into_result(),
            Incoming::Notification(notification) => Err(TerminalError::Protocol(format!(
                "unexpected notification {} while waiting for an answer",
                notification.method
            ))),
        }
    }

    /// Calls `hello` and decodes its result.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`CoreClient::call`].
    pub async fn hello(&self, framed: bool) -> Result<HelloResult, TerminalError> {
        let params = serde_json::json!({
            "generation": crate::detect::CORE_GENERATION,
            "framed": framed,
        });
        let value = self.call("hello", params).await?;
        Ok(serde_json::from_value(value)?)
    }
}

/// Resolves the socket the current environment points at.
///
/// Order: `HC_SOCKET`, then `$XDG_RUNTIME_DIR/herdr-core/<session>.sock`, then
/// a scan of `/run/user` (the per-user runtime directory on this machine).
#[must_use]
pub fn default_socket() -> PathBuf {
    let explicit = std::env::var_os("HC_SOCKET")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    let session = std::env::var("HC_SESSION").unwrap_or_else(|_| "default".to_owned());
    if let Some(path) = resolve_socket(explicit, &session, std::env::var_os("XDG_RUNTIME_DIR")) {
        return path;
    }
    let file = format!("{session}.sock");
    if let Ok(entries) = std::fs::read_dir("/run/user") {
        let mut candidates: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path().join("herdr-core").join(&file))
            .filter(|path| path.exists())
            .collect();
        candidates.sort();
        if let Some(path) = candidates.into_iter().next() {
            return path;
        }
    }
    PathBuf::from("/run/user/0/herdr-core").join(file)
}

fn resolve_socket(
    explicit: Option<PathBuf>,
    session: &str,
    runtime: Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    if let Some(path) = explicit {
        return Some(path);
    }
    runtime.map(|runtime| {
        PathBuf::from(runtime)
            .join("herdr-core")
            .join(format!("{session}.sock"))
    })
}

#[cfg(test)]
mod tests {
    use super::resolve_socket;
    use std::path::PathBuf;

    #[test]
    fn explicit_socket_wins_over_the_runtime_directory() {
        let socket = resolve_socket(
            Some(PathBuf::from("/tmp/custom.sock")),
            "default",
            Some("/run/user/1000".into()),
        );
        assert_eq!(socket, Some(PathBuf::from("/tmp/custom.sock")));
    }

    #[test]
    fn session_names_the_runtime_socket() {
        let socket = resolve_socket(None, "work", Some("/run/user/1000".into()));
        assert_eq!(
            socket,
            Some(PathBuf::from("/run/user/1000/herdr-core/work.sock"))
        );
    }
}
