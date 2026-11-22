//! The herdr-core backend: JSON-RPC over its unix socket.

pub mod attach;
pub mod client;
pub mod map;
pub mod subscribe;
pub mod wire;

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::TerminalBackend;
use crate::detect::{core_problem, core_protocol};
use crate::error::TerminalError;
use crate::model::{
    Access, AgentState, AttachMode, AttachOptions, BackendDescriptor, BackendKind,
    InterruptOutcome, InterruptResult, KeyName, ReadOptions, ScreenText, SendOptions, SendOutcome,
    SpawnSpec, TerminalFilter, TerminalId, TerminalInfo,
};
use crate::stream::{AttachSession, EventStream};

pub use client::CoreClient;

/// The core's own default wait budget for `terminal.send {wait: true}`.
const DEFAULT_WAIT_MS: u64 = 180_000;

/// Extra time on top of the wait budget before the client gives up.
const WAIT_MARGIN: Duration = Duration::from_secs(30);

/// The client deadline for one send: the core's wait budget plus a margin.
fn send_budget(options: &SendOptions) -> Duration {
    if !options.wait_for_settle {
        return crate::util::CALL_TIMEOUT;
    }
    let wait_ms = options.timeout_ms.unwrap_or(DEFAULT_WAIT_MS);
    Duration::from_millis(wait_ms).saturating_add(WAIT_MARGIN)
}

use wire::{
    HelloResult, InterruptResult as WireInterrupt, KeysResult, ListResult, ReadResult, SendResult,
    Terminal,
};

/// A backend for one herdr-core daemon.
#[derive(Debug, Clone)]
pub struct HerdrCoreBackend {
    client: CoreClient,
}

impl HerdrCoreBackend {
    /// Targets an explicit socket path.
    #[must_use]
    pub fn new(socket: impl Into<PathBuf>) -> Self {
        Self {
            client: CoreClient::new(socket),
        }
    }

    /// Targets the socket selected by `HC_SOCKET`, `XDG_RUNTIME_DIR` or a scan.
    #[must_use]
    pub fn from_environment() -> Self {
        Self {
            client: CoreClient::from_environment(),
        }
    }

    /// The socket path in use.
    #[must_use]
    pub fn socket(&self) -> &std::path::Path {
        self.client.socket()
    }

    async fn call<T: DeserializeOwned>(
        &self,
        method: &str,
        params: Value,
    ) -> Result<T, TerminalError> {
        let value = self.client.call(method, params).await?;
        Ok(serde_json::from_value(value)?)
    }

    async fn hello(&self) -> Result<HelloResult, TerminalError> {
        self.client.hello(false).await
    }

    async fn ensure_writable(&self) -> Result<(), TerminalError> {
        let hello = self.hello().await?;
        match core_problem(hello.generation, &hello.version, &hello.features) {
            None => Ok(()),
            Some(problem) => Err(TerminalError::ReadOnly(problem)),
        }
    }
}

#[async_trait]
impl TerminalBackend for HerdrCoreBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::HerdrCore
    }

    async fn detect(&self) -> BackendDescriptor {
        match self.hello().await {
            Ok(hello) => {
                let problem = core_problem(hello.generation, &hello.version, &hello.features);
                BackendDescriptor {
                    kind: BackendKind::HerdrCore,
                    available: true,
                    access: if problem.is_none() {
                        Access::ReadWrite
                    } else {
                        Access::ReadOnly
                    },
                    protocol: Some(core_protocol(hello.generation)),
                    detail: problem,
                    version: Some(hello.version),
                }
            }
            Err(error) => BackendDescriptor::unavailable(BackendKind::HerdrCore, error.to_string()),
        }
    }

    async fn list(&self, filter: &TerminalFilter) -> Result<Vec<TerminalInfo>, TerminalError> {
        let mut params = serde_json::Map::new();
        params.insert("all".to_owned(), Value::Bool(filter.include_finished));
        if !filter.labels.is_empty() {
            params.insert("labels".to_owned(), serde_json::to_value(&filter.labels)?);
        }
        let result: ListResult = self.call("terminal.list", Value::Object(params)).await?;
        Ok(result
            .terminals
            .into_iter()
            .map(map::terminal_info)
            .filter(|info| filter.matches(info))
            .collect())
    }

    async fn get(&self, id: &TerminalId) -> Result<TerminalInfo, TerminalError> {
        let terminal: Terminal = self
            .call("terminal.get", json!({"terminal": id.as_str()}))
            .await?;
        Ok(map::terminal_info(terminal))
    }

    async fn subscribe(&self, filter: &TerminalFilter) -> Result<EventStream, TerminalError> {
        subscribe::subscribe(&self.client, filter).await
    }

    async fn read(
        &self,
        id: &TerminalId,
        options: ReadOptions,
    ) -> Result<ScreenText, TerminalError> {
        let read: ReadResult = self
            .call(
                "terminal.read",
                json!({"terminal": id.as_str(), "lines": options.lines}),
            )
            .await?;
        Ok(map::screen_text(read))
    }

    async fn send_text(
        &self,
        id: &TerminalId,
        text: &str,
        options: SendOptions,
    ) -> Result<SendOutcome, TerminalError> {
        self.ensure_writable().await?;
        let mut params = serde_json::Map::new();
        params.insert("terminal".to_owned(), Value::String(id.as_str().to_owned()));
        params.insert("text".to_owned(), Value::String(text.to_owned()));
        if options.force {
            params.insert("force".to_owned(), Value::Bool(true));
        }
        if let Some(only_if) = options.only_if {
            let word = match only_if {
                AgentState::Idle => "idle",
                AgentState::Working => "working",
                other => {
                    return Err(TerminalError::Unsupported(format!(
                        "herdr-core can only gate a send on idle or working, not {other:?}"
                    )));
                }
            };
            params.insert("if".to_owned(), Value::String(word.to_owned()));
        }
        if options.wait_for_settle {
            params.insert("wait".to_owned(), Value::Bool(true));
        }
        if let Some(timeout_ms) = options.timeout_ms {
            let seconds = std::time::Duration::from_millis(timeout_ms).as_secs_f64();
            params.insert(
                "timeout".to_owned(),
                serde_json::Number::from_f64(seconds).map_or(Value::Null, Value::Number),
            );
        }
        let value = self
            .client
            .call_with_timeout(
                "terminal.send",
                Value::Object(params),
                send_budget(&options),
            )
            .await?;
        let result: SendResult = serde_json::from_value(value)?;
        Ok(SendOutcome {
            started: result.started,
            status: result.status.as_deref().map(AgentState::from_word),
            turn: result.turn,
        })
    }

    async fn send_keys(&self, id: &TerminalId, keys: &[KeyName]) -> Result<(), TerminalError> {
        self.ensure_writable().await?;
        let keys: Vec<&str> = keys.iter().map(KeyName::as_str).collect();
        let _: KeysResult = self
            .call(
                "terminal.keys",
                json!({"terminal": id.as_str(), "keys": keys}),
            )
            .await?;
        Ok(())
    }

    async fn interrupt(&self, id: &TerminalId) -> Result<InterruptOutcome, TerminalError> {
        self.ensure_writable().await?;
        let result: WireInterrupt = self
            .call("terminal.interrupt", json!({"terminal": id.as_str()}))
            .await?;
        let key = KeyName::new(result.key.clone()).unwrap_or_else(|_| KeyName::ctrl_c());
        Ok(InterruptOutcome {
            key,
            result: InterruptResult::from_outcome(&result.outcome),
        })
    }

    async fn kill(&self, id: &TerminalId) -> Result<(), TerminalError> {
        self.ensure_writable().await?;
        let _: Terminal = self
            .call("terminal.kill", json!({"terminal": id.as_str()}))
            .await?;
        Ok(())
    }

    async fn remove(&self, id: &TerminalId) -> Result<(), TerminalError> {
        self.ensure_writable().await?;
        let _ = self
            .client
            .call("terminal.rm", json!({"terminal": id.as_str()}))
            .await?;
        Ok(())
    }

    async fn spawn(&self, spec: &SpawnSpec) -> Result<TerminalInfo, TerminalError> {
        self.ensure_writable().await?;
        let mut params = serde_json::Map::new();
        if spec.line.is_none() && !spec.argv.is_empty() {
            params.insert("argv".to_owned(), serde_json::to_value(&spec.argv)?);
        }
        if let Some(line) = &spec.line {
            params.insert("line".to_owned(), Value::String(line.clone()));
        }
        if let Some(name) = &spec.name {
            params.insert("name".to_owned(), Value::String(name.clone()));
        }
        if let Some(cwd) = &spec.cwd {
            params.insert("cwd".to_owned(), Value::String(cwd.clone()));
        }
        if !spec.labels.is_empty() {
            params.insert("labels".to_owned(), serde_json::to_value(&spec.labels)?);
        }
        if !spec.env.is_empty() {
            params.insert("env".to_owned(), serde_json::to_value(&spec.env)?);
        }
        if let Some(size) = spec.size {
            params.insert("size".to_owned(), serde_json::to_value(size)?);
        }
        if spec.keep {
            params.insert("keep".to_owned(), Value::Bool(true));
        }
        if spec.no_parent {
            params.insert("no_parent".to_owned(), Value::Bool(true));
        }
        if spec.no_notify {
            params.insert("no_notify".to_owned(), Value::Bool(true));
        }
        let terminal: Terminal = self.call("terminal.run", Value::Object(params)).await?;
        Ok(map::terminal_info(terminal))
    }

    async fn attach(
        &self,
        id: &TerminalId,
        options: AttachOptions,
    ) -> Result<AttachSession, TerminalError> {
        if options.mode == AttachMode::Control {
            self.ensure_writable().await?;
        }
        attach::attach(&self.client, id, options).await
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::send_budget;
    use crate::model::SendOptions;

    #[test]
    fn send_deadline_covers_the_cores_wait_budget() {
        let waiting = SendOptions {
            wait_for_settle: true,
            ..SendOptions::default()
        };
        assert_eq!(send_budget(&waiting), Duration::from_secs(210));

        let custom = SendOptions {
            wait_for_settle: true,
            timeout_ms: Some(5_000),
            ..SendOptions::default()
        };
        assert_eq!(send_budget(&custom), Duration::from_secs(35));

        let plain = SendOptions::default();
        assert_eq!(send_budget(&plain), crate::util::CALL_TIMEOUT);
    }
}
