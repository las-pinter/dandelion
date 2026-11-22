//! The classic herdr backend: workspaces, tabs and panes over a line socket.
//!
//! A pane is the closest classic herdr object to a terminal, so each pane
//! becomes one [`TerminalInfo`](crate::TerminalInfo). Workspace and tab labels
//! are projected into `herdr.workspace*`/`herdr.tab*` labels.
//!
//! Classic herdr has no finished panes and no free-form labels. `kill` and
//! `remove` both close the pane, `spawn` ignores labels and initial size, and
//! `interrupt` reports an unknown outcome because the protocol does not say
//! what the key press did.

pub mod attach;
pub mod client;
pub mod map;
pub mod subscribe;
pub mod wire;

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::TerminalBackend;
use crate::detect::{classic_problem, classic_protocol};
use crate::error::TerminalError;
use crate::model::{
    Access, AttachMode, AttachOptions, BackendDescriptor, BackendKind, InterruptOutcome,
    InterruptResult, KeyName, ReadOptions, ScreenText, SendOptions, SendOutcome, SpawnSpec,
    TerminalFilter, TerminalId, TerminalInfo,
};
use crate::stream::{AttachSession, EventStream};

pub use client::ClassicClient;

use wire::{PaneReadResult, Snapshot, WorkspaceCreated};

/// A backend for one classic herdr session.
#[derive(Debug, Clone)]
pub struct ClassicHerdrBackend {
    client: ClassicClient,
    binary: PathBuf,
}

impl ClassicHerdrBackend {
    /// Targets an explicit socket and the `herdr` binary from `PATH`.
    #[must_use]
    pub fn new(socket: impl Into<PathBuf>) -> Self {
        Self {
            client: ClassicClient::new(socket),
            binary: attach::default_binary(),
        }
    }

    /// Targets an explicit socket and binary.
    #[must_use]
    pub fn with_binary(socket: impl Into<PathBuf>, binary: impl Into<PathBuf>) -> Self {
        Self {
            client: ClassicClient::new(socket),
            binary: binary.into(),
        }
    }

    /// Targets the socket selected by `HERDR_SESSION` and the config directory.
    #[must_use]
    pub fn from_environment() -> Self {
        Self {
            client: ClassicClient::from_environment(),
            binary: attach::default_binary(),
        }
    }

    /// The socket path in use.
    #[must_use]
    pub fn socket(&self) -> &Path {
        self.client.socket()
    }

    async fn ensure_writable(&self) -> Result<(), TerminalError> {
        let pong = self.client.ping().await?;
        match classic_problem(pong.protocol, &pong.version) {
            None => Ok(()),
            Some(problem) => Err(TerminalError::ReadOnly(problem)),
        }
    }

    async fn snapshot(&self) -> Result<Snapshot, TerminalError> {
        self.client.snapshot().await
    }
}

#[async_trait]
impl TerminalBackend for ClassicHerdrBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::Herdr
    }

    async fn detect(&self) -> BackendDescriptor {
        match self.client.ping().await {
            Ok(pong) => {
                let problem = classic_problem(pong.protocol, &pong.version);
                BackendDescriptor {
                    kind: BackendKind::Herdr,
                    available: true,
                    version: Some(pong.version.clone()),
                    protocol: Some(classic_protocol(pong.protocol)),
                    access: if problem.is_none() {
                        Access::ReadWrite
                    } else {
                        Access::ReadOnly
                    },
                    detail: problem,
                }
            }
            Err(error) => BackendDescriptor::unavailable(BackendKind::Herdr, error.to_string()),
        }
    }

    async fn list(&self, filter: &TerminalFilter) -> Result<Vec<TerminalInfo>, TerminalError> {
        let snapshot = self.snapshot().await?;
        Ok(map::snapshot_terminals(&snapshot)
            .into_iter()
            .filter(|info| filter.matches(info))
            .collect())
    }

    async fn get(&self, id: &TerminalId) -> Result<TerminalInfo, TerminalError> {
        let snapshot = self.snapshot().await?;
        map::snapshot_terminals(&snapshot)
            .into_iter()
            .find(|info| &info.id == id)
            .ok_or_else(|| TerminalError::NotFound(id.to_string()))
    }

    async fn subscribe(&self, filter: &TerminalFilter) -> Result<EventStream, TerminalError> {
        subscribe::subscribe(&self.client, filter).await
    }

    async fn read(
        &self,
        id: &TerminalId,
        options: ReadOptions,
    ) -> Result<ScreenText, TerminalError> {
        let source = if options.lines > 0 {
            "recent"
        } else {
            "visible"
        };
        let mut params = serde_json::Map::new();
        params.insert("pane_id".to_owned(), Value::String(id.as_str().to_owned()));
        params.insert("source".to_owned(), Value::String(source.to_owned()));
        params.insert(
            "format".to_owned(),
            Value::String(if options.ansi { "ansi" } else { "text" }.to_owned()),
        );
        params.insert("strip_ansi".to_owned(), Value::Bool(!options.ansi));
        if options.lines > 0 {
            params.insert("lines".to_owned(), Value::from(options.lines));
        }
        let value = self.client.call("pane.read", Value::Object(params)).await?;
        let result: PaneReadResult = serde_json::from_value(value)?;
        Ok(map::screen_text(result.read))
    }

    async fn send_text(
        &self,
        id: &TerminalId,
        text: &str,
        _options: SendOptions,
    ) -> Result<SendOutcome, TerminalError> {
        self.ensure_writable().await?;
        let _ = self
            .client
            .call(
                "pane.send_input",
                json!({"pane_id": id.as_str(), "text": text, "keys": ["enter"]}),
            )
            .await?;
        Ok(SendOutcome {
            started: None,
            status: None,
            turn: 0,
        })
    }

    async fn send_keys(&self, id: &TerminalId, keys: &[KeyName]) -> Result<(), TerminalError> {
        self.ensure_writable().await?;
        let keys: Vec<&str> = keys.iter().map(KeyName::as_str).collect();
        let _ = self
            .client
            .call(
                "pane.send_keys",
                json!({"pane_id": id.as_str(), "keys": keys}),
            )
            .await?;
        Ok(())
    }

    async fn interrupt(&self, id: &TerminalId) -> Result<InterruptOutcome, TerminalError> {
        self.ensure_writable().await?;
        let _ = self
            .client
            .call(
                "pane.send_keys",
                json!({"pane_id": id.as_str(), "keys": ["ctrl+c"]}),
            )
            .await?;
        Ok(InterruptOutcome {
            key: KeyName::ctrl_c(),
            result: InterruptResult::Unknown,
        })
    }

    async fn kill(&self, id: &TerminalId) -> Result<(), TerminalError> {
        self.ensure_writable().await?;
        let _ = self
            .client
            .call("pane.close", json!({"pane_id": id.as_str()}))
            .await?;
        Ok(())
    }

    async fn remove(&self, id: &TerminalId) -> Result<(), TerminalError> {
        self.ensure_writable().await?;
        match self.get(id).await {
            Err(TerminalError::NotFound(_)) => Ok(()),
            Err(error) => Err(error),
            Ok(_) => {
                let _ = self
                    .client
                    .call("pane.close", json!({"pane_id": id.as_str()}))
                    .await?;
                Ok(())
            }
        }
    }

    async fn spawn(&self, spec: &SpawnSpec) -> Result<TerminalInfo, TerminalError> {
        self.ensure_writable().await?;
        let mut params = serde_json::Map::new();
        if let Some(cwd) = &spec.cwd {
            params.insert("cwd".to_owned(), Value::String(cwd.clone()));
        }
        if let Some(name) = &spec.name {
            params.insert("label".to_owned(), Value::String(name.clone()));
        }
        if !spec.env.is_empty() {
            params.insert("env".to_owned(), serde_json::to_value(&spec.env)?);
        }
        params.insert("focus".to_owned(), Value::Bool(false));
        let value = self
            .client
            .call("workspace.create", Value::Object(params))
            .await?;
        let created: WorkspaceCreated = serde_json::from_value(value)?;
        let command = spec
            .line
            .clone()
            .or_else(|| (!spec.argv.is_empty()).then(|| shell_join(&spec.argv)));
        if let Some(command) = command {
            let _ = self
                .client
                .call(
                    "pane.send_input",
                    json!({
                        "pane_id": created.root_pane.pane_id,
                        "text": command,
                        "keys": ["enter"],
                    }),
                )
                .await?;
        }
        Ok(map::pane_info(
            &created.root_pane,
            Some(&created.workspace),
            Some(&created.tab),
            None,
        ))
    }

    async fn attach(
        &self,
        id: &TerminalId,
        options: AttachOptions,
    ) -> Result<AttachSession, TerminalError> {
        if options.mode == AttachMode::Control {
            self.ensure_writable().await?;
        }
        attach::attach(&self.binary, id, options)
    }
}

/// Joins `argv` into a POSIX shell line, quoting every word.
#[must_use]
pub fn shell_join(argv: &[String]) -> String {
    argv.iter()
        .map(|argument| format!("'{}'", argument.replace('\'', "'\\''")))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};

    use serde_json::{Value, json};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::UnixListener;

    use super::{ClassicHerdrBackend, shell_join};
    use crate::TerminalBackend;
    use crate::model::{KeyName, SendOptions, SpawnSpec, TerminalId};

    #[test]
    fn argv_is_quoted_for_the_shell() {
        let line = shell_join(&[
            "bash".to_owned(),
            "--norc".to_owned(),
            "say hi".to_owned(),
            "it's".to_owned(),
        ]);
        assert_eq!(line, "'bash' '--norc' 'say hi' 'it'\\''s'");
    }

    fn socket_path(tag: &str) -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "dandelion-terminals-{tag}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir.join("herdr.sock")
    }

    fn reply_for(method: &str) -> Value {
        match method {
            "ping" => json!({"type": "pong", "version": "0.9.5", "protocol": 22}),
            "workspace.create" => json!({
                "type": "workspace_created",
                "workspace": {"workspace_id": "w9", "number": 9, "label": "probe",
                              "focused": false, "pane_count": 1, "tab_count": 1,
                              "active_tab_id": "w9:t1", "agent_status": "unknown"},
                "tab": {"tab_id": "w9:t1", "workspace_id": "w9", "number": 1,
                        "label": "main", "focused": false, "pane_count": 1,
                        "agent_status": "unknown"},
                "root_pane": {"pane_id": "w9:p1", "workspace_id": "w9",
                              "tab_id": "w9:t1", "agent_status": "unknown",
                              "cwd": "/home/dev"}
            }),
            "session.snapshot" => json!({
                "type": "session_snapshot",
                "snapshot": {
                    "version": "0.9.5", "protocol": 22,
                    "workspaces": [], "tabs": [], "agents": [], "layouts": [],
                    "panes": [{"pane_id": "w1:p1", "workspace_id": "w1",
                               "tab_id": "w1:t1", "agent_status": "idle"}]
                }
            }),
            _ => json!({"type": "ok"}),
        }
    }

    #[allow(clippy::too_many_lines)]
    #[tokio::test]
    async fn write_paths_send_the_documented_methods() {
        let path = socket_path("classic-writes");
        let listener = UnixListener::bind(&path).expect("bind");
        let recorded: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&recorded);
        let server = tokio::spawn(async move {
            for _ in 0..14 {
                let (stream, _) = listener.accept().await.expect("accept");
                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                reader.read_line(&mut line).await.expect("request");
                let request: Value = serde_json::from_str(&line).expect("json");
                let method = request["method"].as_str().expect("method").to_owned();
                seen.lock().expect("record").push(request.clone());
                let mut stream = reader.into_inner();
                let mut reply = json!({
                    "id": request["id"],
                    "result": reply_for(&method),
                })
                .to_string();
                reply.push('\n');
                stream.write_all(reply.as_bytes()).await.expect("reply");
            }
        });

        let backend = ClassicHerdrBackend::new(&path);
        let spawned = backend
            .spawn(
                &SpawnSpec::new()
                    .line("echo hi")
                    .named("probe")
                    .label("purpose", "dandelion-test"),
            )
            .await
            .expect("spawn");
        assert_eq!(spawned.id.as_str(), "w9:p1");
        assert_eq!(
            spawned.labels.get("herdr.workspace").map(String::as_str),
            Some("probe")
        );

        let target = TerminalId::new("w1:p1");
        backend
            .send_text(&target, "hello", SendOptions::default())
            .await
            .expect("send text");
        let keys = [KeyName::new("ctrl+c").expect("key")];
        backend.send_keys(&target, &keys).await.expect("send keys");
        let interrupt = backend.interrupt(&target).await.expect("interrupt");
        assert_eq!(interrupt.key, KeyName::ctrl_c());
        backend.kill(&target).await.expect("kill");
        backend.remove(&target).await.expect("remove");

        server.await.expect("server");
        let requests = recorded.lock().expect("record").clone();
        let methods: Vec<&str> = requests
            .iter()
            .filter_map(|request| request["method"].as_str())
            .collect();
        assert_eq!(
            methods,
            [
                "ping",
                "workspace.create",
                "pane.send_input",
                "ping",
                "pane.send_input",
                "ping",
                "pane.send_keys",
                "ping",
                "pane.send_keys",
                "ping",
                "pane.close",
                "ping",
                "session.snapshot",
                "pane.close",
            ]
        );
        assert_eq!(requests[1]["params"]["label"], "probe");
        assert_eq!(requests[2]["params"]["text"], "echo hi");
        assert_eq!(requests[2]["params"]["keys"][0], "enter");
        assert_eq!(requests[4]["params"]["text"], "hello");
        assert_eq!(requests[6]["params"]["keys"][0], "ctrl+c");
        assert_eq!(requests[8]["params"]["keys"][0], "ctrl+c");
        assert_eq!(requests[10]["params"]["pane_id"], "w1:p1");
        assert_eq!(requests[13]["params"]["pane_id"], "w1:p1");

        let _ = std::fs::remove_dir_all(path.parent().expect("parent"));
    }
}
