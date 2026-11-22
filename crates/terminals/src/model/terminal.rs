//! Terminal summaries and backend detection types.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{AgentKind, AgentStatus, TerminalId};

/// Whether the program in a terminal is still running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TerminalLifecycle {
    /// The program is running.
    Live,
    /// The program ended; the terminal may still be readable.
    Finished,
}

/// Terminal columns and rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TerminalSize {
    /// Columns.
    pub cols: u16,
    /// Rows.
    pub rows: u16,
}

impl TerminalSize {
    /// The conventional fallback size.
    pub const DEFAULT: Self = Self { cols: 80, rows: 24 };

    /// Creates a size.
    #[must_use]
    pub const fn new(cols: u16, rows: u16) -> Self {
        Self { cols, rows }
    }
}

impl Default for TerminalSize {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// One terminal in the common model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TerminalInfo {
    /// Backend address of the terminal.
    pub id: TerminalId,
    /// Human-readable title, falling back to the name or address.
    pub title: String,
    /// Current working directory, when the backend knows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Free-form labels. Classic herdr reports structural ids here.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,
    /// The agent kind, when the program identified itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_kind: Option<AgentKind>,
    /// The agent's own state, when the program reported one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_state: Option<AgentStatus>,
    /// Whether the program is still running.
    pub lifecycle: TerminalLifecycle,
    /// Creation time in milliseconds since the epoch, when reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_ms: Option<u64>,
    /// Exit code, once finished.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Why the program ended, once finished.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_reason: Option<String>,
}

impl TerminalInfo {
    /// Returns true when the terminal is live.
    #[must_use]
    pub fn is_live(&self) -> bool {
        self.lifecycle == TerminalLifecycle::Live
    }
}

/// Which backends exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    /// herdr-core, the `hc` daemon.
    HerdrCore,
    /// Classic herdr, the older multiplexer.
    Herdr,
}

impl BackendKind {
    /// Returns the stable machine name.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HerdrCore => "herdr-core",
            Self::Herdr => "herdr",
        }
    }
}

impl fmt::Display for BackendKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The protocol generation a backend speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "backend", rename_all = "snake_case")]
pub enum ProtocolId {
    /// herdr-core handshake generation.
    HerdrCore {
        /// `hello.generation`.
        generation: u32,
    },
    /// Classic herdr's private protocol number.
    Herdr {
        /// `ping.protocol`.
        protocol: u32,
    },
}

/// Whether a backend may be written to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    /// Reads and writes are allowed.
    ReadWrite,
    /// Only reads are allowed because the version is unknown.
    ReadOnly,
}

/// What detection learned about one backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct BackendDescriptor {
    /// Which backend this describes.
    pub kind: BackendKind,
    /// Whether it answered.
    pub available: bool,
    /// The version it reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The protocol it speaks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<ProtocolId>,
    /// Read-write or read-only.
    pub access: Access,
    /// Why a backend is unavailable or read-only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl BackendDescriptor {
    /// Builds an unavailable descriptor.
    #[must_use]
    pub fn unavailable(kind: BackendKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            available: false,
            version: None,
            protocol: None,
            access: Access::ReadOnly,
            detail: Some(detail.into()),
        }
    }
}
