//! Agent listing and control over a terminal backend.
//!
//! herdr-core is tried first, then classic herdr; with neither reachable the
//! snapshot reports the page as unavailable instead of failing, and a
//! background retry adopts a backend that starts after the server. Agents are
//! terminals that report an agent kind. State changes ride the `agents` hub
//! topic; output rides a per-agent attach socket, because the hub carries
//! JSON only. Every mutating request writes exactly one audit row; the typed
//! prompt, the answer text, and the keys are never audited.

mod backend;
mod publisher;
pub mod routes;
mod ws;

pub use backend::spawn_backend_retry;
pub use publisher::{spawn_publisher, spawn_publisher_with};
pub use routes::router;

use std::sync::{Arc, PoisonError, RwLock};

use dandelion_terminals::{
    AttachMode, AttachOptions, AttachSession, BackendDescriptor, KeyName, ReadOptions, ScreenText,
    SendOptions, SpawnSpec, TerminalBackend, TerminalError, TerminalFilter, TerminalId,
    TerminalInfo, TerminalSize,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::claude::ClaudeAgent;

/// The hub topic carrying agent state changes.
pub const TOPIC: &str = "agents";
/// History lines a new attach asks for.
const ATTACH_HISTORY: u32 = 200;
/// Viewport an observer asks for, in cells. An observer never resizes the
/// real terminal, so this only seeds the stream's first paint.
const ATTACH_SIZE: TerminalSize = TerminalSize {
    cols: 120,
    rows: 40,
};

/// What the terminal backend reports for the Agents page.
#[derive(Debug, Clone, Serialize)]
pub struct AgentsListing {
    /// Whether a backend answered.
    pub available: bool,
    /// The backend and its version, when one answered.
    pub backend: Option<BackendDescriptor>,
    /// Every live terminal that reports an agent.
    pub agents: Vec<TerminalInfo>,
}

/// What the Agents page needs in one response.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct AgentsSnapshot {
    /// Whether a backend answered.
    pub available: bool,
    /// The backend and its version, when one answered.
    pub backend: Option<BackendDescriptor>,
    /// Every live terminal that reports an agent.
    pub agents: Vec<TerminalInfo>,
    /// Claude Code sessions, when that state directory is readable.
    pub claude: Vec<ClaudeAgent>,
}

impl AgentsSnapshot {
    /// Combines the backend listing and the Claude cache.
    ///
    /// This is the only constructor, so a snapshot cannot leave `claude`
    /// looking complete when it was never filled.
    #[must_use]
    pub fn compose(listing: AgentsListing, claude: Vec<ClaudeAgent>) -> Self {
        Self {
            available: listing.available,
            backend: listing.backend,
            agents: listing.agents,
            claude,
        }
    }
}

/// A request to act on one agent's terminal.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct AgentActionRequest {
    /// Backend terminal id, as listed by `GET /api/agents`.
    pub terminal: String,
    /// What to do.
    pub action: AgentAction,
}

/// One agent action.
#[derive(Debug, Clone, Deserialize, Serialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentAction {
    /// Types a prompt into the agent and submits it.
    Prompt {
        /// The text to send.
        text: String,
    },
    /// Presses named keys, such as `enter` or `escape`.
    Keys {
        /// Key names the crate understands.
        keys: Vec<String>,
    },
    /// Types an answer into a dialog that is blocking the agent.
    ///
    /// Unlike [`Self::Prompt`] this is forced: herdr refuses a send to a
    /// blocked program unless the caller says it is answering the block.
    Answer {
        /// The answer text.
        text: String,
    },
    /// Answers a permission prompt with Enter.
    Approve,
    /// Answers a permission prompt with Escape.
    Deny,
    /// Interrupts the running turn.
    Interrupt,
    /// Ends the terminal and its program.
    Kill,
}

impl AgentAction {
    /// The stable name stored in the audit detail.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Prompt { .. } => "prompt",
            Self::Keys { .. } => "keys",
            Self::Answer { .. } => "answer",
            Self::Approve => "approve",
            Self::Deny => "deny",
            Self::Interrupt => "interrupt",
            Self::Kill => "kill",
        }
    }
}

/// A request to start an agent in a directory.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct SpawnAgentRequest {
    /// The shell line to run, such as `claude`.
    pub line: String,
    /// Working directory, usually a project or worktree path.
    pub cwd: Option<String>,
    /// Tab name; the backend requires uniqueness among live terminals.
    pub name: Option<String>,
}

/// Why an agent request failed.
#[derive(Debug, thiserror::Error)]
pub enum AgentsError {
    /// No terminal backend is available at request time.
    #[error("no terminal backend is available")]
    Unavailable,
    /// An action arrived that needs a person's eye first.
    #[error("the agent is waiting on a permission prompt")]
    Blocked,
    /// The backend refused or failed.
    #[error(transparent)]
    Backend(#[from] TerminalError),
}

/// The backend in force.
///
/// Chosen at startup, or installed once when the retry adopts a backend that
/// answered late. Handlers read it on a request path, so the lock only guards
/// a pointer swap and is never held across an await.
#[derive(Clone, Default)]
pub struct Agents {
    backend: Arc<RwLock<Option<Arc<dyn TerminalBackend>>>>,
}

impl std::fmt::Debug for Agents {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Agents")
            .field("available", &self.backend().is_some())
            .finish()
    }
}

impl Agents {
    /// Picks herdr-core, then classic herdr, keeping the server up with
    /// neither. [`spawn_backend_retry`] covers a backend that starts later.
    pub async fn connect() -> Self {
        let agents = Self::default();
        if let Some(backend) = backend::detect().await {
            agents.install(backend);
        } else {
            tracing::warn!("no terminal backend is available at startup");
        }
        agents
    }

    /// Uses an explicit backend; tests inject a fake.
    #[must_use]
    pub fn with_backend(backend: Arc<dyn TerminalBackend>) -> Self {
        let agents = Self::default();
        agents.install(backend);
        agents
    }

    /// A disabled service with no backend.
    #[must_use]
    pub fn disabled() -> Self {
        Self::default()
    }

    /// Adopts a backend discovered after startup.
    ///
    /// Only the boot retry and tests call this, once, on an empty slot; the
    /// retry's own guard keeps a live backend from being replaced. Overwriting
    /// is possible here because the slot has no owner to hand the old value
    /// back to.
    #[doc(hidden)]
    pub fn install(&self, backend: Arc<dyn TerminalBackend>) {
        let mut slot = self.backend.write().unwrap_or_else(PoisonError::into_inner);
        *slot = Some(backend);
    }

    /// The backend, when one is available.
    #[must_use]
    pub fn backend(&self) -> Option<Arc<dyn TerminalBackend>> {
        self.backend
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Every live terminal with an agent, plus the backend's descriptor.
    ///
    /// Without a backend the listing reports `available: false` rather than
    /// failing, so the page still renders.
    ///
    /// # Errors
    ///
    /// Returns the backend's error when the listing fails.
    pub async fn snapshot(&self) -> Result<AgentsListing, AgentsError> {
        let Some(backend) = self.backend() else {
            return Ok(AgentsListing {
                available: false,
                backend: None,
                agents: Vec::new(),
            });
        };
        let descriptor = backend.detect().await;
        if !descriptor.available {
            return Ok(AgentsListing {
                available: false,
                backend: Some(descriptor),
                agents: Vec::new(),
            });
        }
        let terminals = backend.list(&TerminalFilter::all()).await?;
        let mut agents: Vec<TerminalInfo> = terminals
            .into_iter()
            .filter(|terminal| terminal.agent_kind.is_some())
            .collect();
        agents.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(AgentsListing {
            available: true,
            backend: Some(descriptor),
            agents,
        })
    }

    /// The agent's visible screen.
    ///
    /// History is never read here: herdr-core scrolls a full-screen program to
    /// stitch it, which counts as typing into that program.
    ///
    /// # Errors
    ///
    /// Returns [`AgentsError::Unavailable`] without a backend and the
    /// backend's error when the read fails.
    pub async fn screen(&self, terminal: &str) -> Result<ScreenText, AgentsError> {
        let backend = self.backend().ok_or(AgentsError::Unavailable)?;
        Ok(backend
            .read(&TerminalId::new(terminal), ReadOptions::screen())
            .await?)
    }

    /// Runs one action, returning the new terminal for a spawn.
    ///
    /// # Errors
    ///
    /// Returns [`AgentsError::Unavailable`] without a backend,
    /// [`AgentsError::Blocked`] when an action needs a person first, and the
    /// backend's error otherwise.
    pub async fn act(&self, request: &AgentActionRequest) -> Result<(), AgentsError> {
        let backend = self.backend().ok_or(AgentsError::Unavailable)?;
        let id = TerminalId::new(request.terminal.clone());
        match &request.action {
            AgentAction::Prompt { text } => {
                backend
                    .send_text(&id, text, SendOptions::default())
                    .await
                    .map_err(blocked)?;
            }
            AgentAction::Keys { keys } => {
                let keys = keys
                    .iter()
                    .map(|key| KeyName::new(key.clone()))
                    .collect::<Result<Vec<_>, _>>()?;
                backend.send_keys(&id, &keys).await?;
            }
            AgentAction::Answer { text } => {
                let options = SendOptions {
                    force: true,
                    ..SendOptions::default()
                };
                backend.send_text(&id, text, options).await?;
            }
            AgentAction::Approve => {
                backend.send_keys(&id, &[KeyName::enter()]).await?;
            }
            AgentAction::Deny => {
                backend.send_keys(&id, &[KeyName::escape()]).await?;
            }
            AgentAction::Interrupt => {
                backend.interrupt(&id).await?;
            }
            AgentAction::Kill => {
                backend.kill(&id).await?;
            }
        }
        Ok(())
    }

    /// Starts an agent terminal in a directory.
    ///
    /// # Errors
    ///
    /// Returns [`AgentsError::Unavailable`] without a backend and the
    /// backend's error when the spawn fails.
    pub async fn spawn(&self, request: &SpawnAgentRequest) -> Result<TerminalInfo, AgentsError> {
        let backend = self.backend().ok_or(AgentsError::Unavailable)?;
        let mut spec = SpawnSpec::new().line(request.line.clone());
        spec.cwd.clone_from(&request.cwd);
        if let Some(name) = &request.name {
            spec.name = Some(name.clone());
        }
        Ok(backend.spawn(&spec).await?)
    }

    /// Opens the observe attach session for one agent.
    ///
    /// The pane only watches. A control attach resizes the agent's real
    /// terminal, which belongs to herdr-app and the agent, so typing and
    /// approval go through the audited action API instead.
    ///
    /// # Errors
    ///
    /// Returns [`AgentsError::Unavailable`] without a backend and the
    /// backend's error when the attach fails.
    pub async fn attach(&self, terminal: &str) -> Result<AttachSession, AgentsError> {
        let backend = self.backend().ok_or(AgentsError::Unavailable)?;
        let options = AttachOptions {
            mode: AttachMode::Observe,
            size: ATTACH_SIZE,
            history: ATTACH_HISTORY,
        };
        Ok(backend.attach(&TerminalId::new(terminal), options).await?)
    }
}

fn blocked(error: TerminalError) -> AgentsError {
    match error {
        TerminalError::Blocked(reason) => {
            tracing::debug!(%reason, "the agent refused input while blocked");
            AgentsError::Blocked
        }
        other => AgentsError::Backend(other),
    }
}

/// Maps a backend error to a stable code, an HTTP status, and a message.
pub(crate) fn error_code_and_status(error: &AgentsError) -> (&'static str, axum::http::StatusCode) {
    use axum::http::StatusCode;
    match error {
        AgentsError::Unavailable | AgentsError::Backend(TerminalError::Unavailable(_)) => {
            ("unavailable", StatusCode::SERVICE_UNAVAILABLE)
        }
        AgentsError::Backend(TerminalError::NotFound(_)) => ("not_found", StatusCode::NOT_FOUND),
        AgentsError::Backend(TerminalError::InvalidKey(_)) => {
            ("invalid_key", StatusCode::BAD_REQUEST)
        }
        AgentsError::Blocked
        | AgentsError::Backend(TerminalError::Blocked(_) | TerminalError::StatusMismatch(_)) => {
            ("agent_blocked", StatusCode::CONFLICT)
        }
        AgentsError::Backend(TerminalError::ReadOnly(_) | TerminalError::Unsupported(_)) => {
            ("unsupported", StatusCode::CONFLICT)
        }
        AgentsError::Backend(TerminalError::Timeout) => ("timeout", StatusCode::GATEWAY_TIMEOUT),
        AgentsError::Backend(_) => ("backend_error", StatusCode::BAD_GATEWAY),
    }
}
