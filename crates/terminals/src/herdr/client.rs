//! A line-JSON client for classic herdr's public socket.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::time::timeout;

use crate::error::TerminalError;
use crate::util::CALL_TIMEOUT;

use super::wire::{LineError, LineResponse, Pong, Snapshot};

/// A one-request-per-connection client.
#[derive(Debug, Clone)]
pub struct ClassicClient {
    socket: Arc<PathBuf>,
    next_id: Arc<AtomicU64>,
}

impl ClassicClient {
    /// Targets an explicit socket path.
    #[must_use]
    pub fn new(socket: impl Into<PathBuf>) -> Self {
        Self {
            socket: Arc::new(socket.into()),
            next_id: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Targets the default session socket.
    #[must_use]
    pub fn from_environment() -> Self {
        Self::new(default_socket())
    }

    /// The socket path in use.
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

    /// Performs one request and returns its result object.
    ///
    /// # Errors
    ///
    /// Returns the server's error, [`TerminalError::Timeout`], or
    /// [`TerminalError::Protocol`] when the answer cannot be read.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, TerminalError> {
        let mut stream = self.connect().await?;
        let id = format!("dandelion:{}", self.next_id.fetch_add(1, Ordering::Relaxed));
        let request = serde_json::json!({"id": id, "method": method, "params": params});
        stream
            .write_all(serde_json::to_string(&request)?.as_bytes())
            .await?;
        stream.write_all(b"\n").await?;
        stream.flush().await?;

        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        let read = timeout(CALL_TIMEOUT, reader.read_line(&mut line))
            .await
            .map_err(|_| TerminalError::Timeout)??;
        if read == 0 {
            return Err(TerminalError::Protocol(
                "herdr closed the connection without an answer".into(),
            ));
        }
        decode_response(&line)?.into_result()
    }

    /// Calls `ping`.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`ClassicClient::call`].
    pub async fn ping(&self) -> Result<Pong, TerminalError> {
        let value = self.call("ping", serde_json::json!({})).await?;
        from_result(value)
    }

    /// Calls `session.snapshot`.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`ClassicClient::call`].
    pub async fn snapshot(&self) -> Result<Snapshot, TerminalError> {
        let value = self.call("session.snapshot", serde_json::json!({})).await?;
        let result: SnapshotResult = from_result(value)?;
        Ok(result.snapshot)
    }

    /// Opens a subscription connection with the given subscription list.
    pub async fn subscribe_connection(
        &self,
        subscriptions: Value,
    ) -> Result<(BufReader<UnixStream>, String), TerminalError> {
        let mut stream = self.connect().await?;
        let id = format!("sub:{}", self.next_id.fetch_add(1, Ordering::Relaxed));
        let request = serde_json::json!({
            "id": id,
            "method": "events.subscribe",
            "params": {"subscriptions": subscriptions},
        });
        stream
            .write_all(serde_json::to_string(&request)?.as_bytes())
            .await?;
        stream.write_all(b"\n").await?;
        stream.flush().await?;
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        let read = timeout(CALL_TIMEOUT, reader.read_line(&mut line))
            .await
            .map_err(|_| TerminalError::Timeout)??;
        if read == 0 {
            return Err(TerminalError::Protocol(
                "herdr closed the subscription without an answer".into(),
            ));
        }
        let response = decode_response(&line)?.into_result()?;
        let kind = response
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if kind != "subscription_started" {
            return Err(TerminalError::Protocol(format!(
                "expected subscription_started, got {kind}"
            )));
        }
        Ok((reader, id))
    }
}

#[derive(Debug, serde::Deserialize)]
struct SnapshotResult {
    snapshot: Snapshot,
}

fn from_result<T: DeserializeOwned>(value: Value) -> Result<T, TerminalError> {
    Ok(serde_json::from_value(value)?)
}

fn decode_response(line: &str) -> Result<LineResponse, TerminalError> {
    let response: LineResponse = serde_json::from_str(line)?;
    Ok(response)
}

impl LineResponse {
    fn into_result(self) -> Result<Value, TerminalError> {
        if let Some(error) = self.error {
            return Err(map_error(error));
        }
        self.result
            .ok_or_else(|| TerminalError::Protocol("response has no result".into()))
    }
}

fn map_error(error: LineError) -> TerminalError {
    match error.code.as_str() {
        "server_not_running" => TerminalError::Unavailable(error.message),
        "not_found" => TerminalError::NotFound(error.message),
        other => TerminalError::Backend {
            code: None,
            kind: Some(other.to_owned()),
            message: error.message,
            hint: None,
        },
    }
}

/// Resolves the classic herdr socket for a session.
///
/// `$HERDR_SESSION` names a non-default session; the default lives directly in
/// `$XDG_CONFIG_HOME/herdr` (or `~/.config/herdr`).
#[must_use]
pub fn default_socket() -> PathBuf {
    let session = std::env::var("HERDR_SESSION").ok();
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"));
    socket_for(&config, session.as_deref())
}

fn socket_for(config: &Path, session: Option<&str>) -> PathBuf {
    let base = config.join("herdr");
    match session.filter(|session| !session.is_empty() && *session != "default") {
        Some(session) => base.join("sessions").join(session).join("herdr.sock"),
        None => base.join("herdr.sock"),
    }
}

#[cfg(test)]
mod tests {
    use super::{decode_response, socket_for};
    use std::path::Path;

    #[test]
    fn default_and_named_sessions_live_in_different_places() {
        let config = Path::new("/home/dev/.config");
        assert_eq!(
            socket_for(config, None),
            Path::new("/home/dev/.config/herdr/herdr.sock")
        );
        assert_eq!(
            socket_for(config, Some("default")),
            Path::new("/home/dev/.config/herdr/herdr.sock")
        );
        assert_eq!(
            socket_for(config, Some("work")),
            Path::new("/home/dev/.config/herdr/sessions/work/herdr.sock")
        );
    }

    #[test]
    fn result_type_is_preserved_verbatim() {
        let line = r#"{"id":"1","result":{"type":"pong","version":"0.9.5","protocol":22}}"#;
        let response = decode_response(line).expect("response");
        let result = response.result.expect("result");
        assert_eq!(result["type"], "pong");
        assert_eq!(result["protocol"], 22);
    }

    #[test]
    fn recorded_ping_keeps_the_protocol_read_write() {
        let pong = crate::test_support::classic_pong();
        assert_eq!(pong.protocol, 22);
        assert_eq!(pong.version, "0.9.5");
        assert_eq!(
            crate::detect::classic_problem(pong.protocol, &pong.version),
            None
        );
    }

    #[tokio::test]
    async fn call_writes_one_request_line_and_reads_one_result() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        use tokio::net::UnixListener;

        use crate::herdr::wire::LineResponse;

        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "dandelion-terminals-classic-client-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("herdr.sock");
        let listener = UnixListener::bind(&path).expect("bind");
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("accept");
            let (read, mut write) = stream.split();
            let mut reader = BufReader::new(read);
            let mut line = String::new();
            reader.read_line(&mut line).await.expect("request line");
            assert!(line.ends_with('\n'));
            let request: serde_json::Value = serde_json::from_str(&line).expect("request json");
            assert_eq!(request["method"], "ping");
            assert!(request["params"].is_object());
            let id = request["id"].as_str().expect("string id");
            assert!(id.starts_with("dandelion:"));
            let mut reply = serde_json::json!({
                "id": id,
                "result": {"type": "pong", "version": "0.9.5", "protocol": 22},
            })
            .to_string();
            reply.push('\n');
            write.write_all(reply.as_bytes()).await.expect("reply");
        });

        let client = super::ClassicClient::new(&path);
        let pong = client.ping().await.expect("ping");
        assert_eq!(pong.version, "0.9.5");
        assert_eq!(pong.protocol, 22);
        server.await.expect("server");
        let _ = std::fs::remove_dir_all(&dir);

        // The fixture keeps the same shape the live server answered with.
        let fixture: LineResponse = serde_json::from_str(
            r#"{"id":"1","result":{"type":"pong","version":"0.9.5","protocol":22}}"#,
        )
        .expect("fixture line");
        assert!(fixture.error.is_none());
    }

    #[test]
    fn server_errors_become_domain_errors() {
        let line = r#"{"id":"x","error":{"code":"server_not_running","message":"not running"}}"#;
        let response = decode_response(line).expect("response");
        let error = response.into_result().expect_err("error");
        assert!(matches!(error, crate::error::TerminalError::Unavailable(_)));
    }
}
