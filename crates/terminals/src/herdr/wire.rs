//! Wire types for classic herdr 0.9.x.
//!
//! Classic herdr speaks line-delimited JSON with one request per connection and
//! a string id. Events use `{"event": ..., "data": ...}` frames, while the
//! terminal stream is NDJSON with base64 payloads.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A response line from `herdr.sock`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineResponse {
    /// Echoed request id.
    #[serde(default)]
    pub id: Option<String>,
    /// Result object, whose `type` names the shape.
    #[serde(default)]
    pub result: Option<Value>,
    /// Server error.
    #[serde(default)]
    pub error: Option<LineError>,
}

/// A classic herdr error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineError {
    /// Stable error code.
    pub code: String,
    /// Human-readable message.
    pub message: String,
}

/// The `ping` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pong {
    /// Server version.
    pub version: String,
    /// Private protocol number.
    pub protocol: u32,
}

/// A workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    /// Workspace id such as `w1`.
    pub workspace_id: String,
    /// Display number.
    #[serde(default)]
    pub number: u32,
    /// Label.
    #[serde(default)]
    pub label: String,
    /// Whether this workspace is focused.
    #[serde(default)]
    pub focused: bool,
    /// Number of panes.
    #[serde(default)]
    pub pane_count: u32,
    /// Number of tabs.
    #[serde(default)]
    pub tab_count: u32,
    /// Focused tab id.
    #[serde(default)]
    pub active_tab_id: String,
    /// Aggregate agent status.
    #[serde(default)]
    pub agent_status: String,
    /// Metadata tokens.
    #[serde(default)]
    pub tokens: BTreeMap<String, String>,
}

/// A tab.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tab {
    /// Tab id such as `w1:t1`.
    pub tab_id: String,
    /// Owning workspace.
    pub workspace_id: String,
    /// Display number.
    #[serde(default)]
    pub number: u32,
    /// Label.
    #[serde(default)]
    pub label: String,
    /// Whether this tab is focused.
    #[serde(default)]
    pub focused: bool,
    /// Number of panes.
    #[serde(default)]
    pub pane_count: u32,
    /// Aggregate agent status.
    #[serde(default)]
    pub agent_status: String,
}

/// Scroll position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scroll {
    /// Lines scrolled up.
    #[serde(default)]
    pub offset_from_bottom: u64,
    /// Maximum scrollable distance.
    #[serde(default)]
    pub max_offset_from_bottom: u64,
    /// Visible rows.
    #[serde(default)]
    pub viewport_rows: u64,
}

/// A pane, which the common model treats as a terminal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pane {
    /// Pane id such as `w1:p2`.
    pub pane_id: String,
    /// Terminal id.
    #[serde(default)]
    pub terminal_id: String,
    /// Owning workspace.
    pub workspace_id: String,
    /// Owning tab.
    pub tab_id: String,
    /// Whether this pane is focused.
    #[serde(default)]
    pub focused: bool,
    /// Detected agent kind.
    #[serde(default)]
    pub agent: Option<String>,
    /// Pretty agent name.
    #[serde(default)]
    pub display_agent: Option<String>,
    /// Agent status word.
    #[serde(default)]
    pub agent_status: String,
    /// Shell working directory.
    #[serde(default)]
    pub cwd: Option<String>,
    /// Foreground program's working directory.
    #[serde(default)]
    pub foreground_cwd: Option<String>,
    /// Pane title.
    #[serde(default)]
    pub title: Option<String>,
    /// Terminal title.
    #[serde(default)]
    pub terminal_title: Option<String>,
    /// Terminal title with agent decoration removed.
    #[serde(default)]
    pub terminal_title_stripped: Option<String>,
    /// Pane label.
    #[serde(default)]
    pub label: Option<String>,
    /// Metadata labels the agent or user published.
    #[serde(default)]
    pub state_labels: BTreeMap<String, String>,
    /// Pane revision; semantics are not documented.
    #[serde(default)]
    pub revision: u64,
    /// Scroll position.
    #[serde(default)]
    pub scroll: Option<Scroll>,
}

/// An agent record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agent {
    /// Terminal id.
    #[serde(default)]
    pub terminal_id: String,
    /// Agent kind.
    #[serde(default)]
    pub agent: Option<String>,
    /// Pretty agent name.
    #[serde(default)]
    pub display_agent: Option<String>,
    /// Agent status word.
    #[serde(default)]
    pub agent_status: String,
    /// Owning workspace.
    #[serde(default)]
    pub workspace_id: String,
    /// Owning tab.
    #[serde(default)]
    pub tab_id: String,
    /// Owning pane.
    #[serde(default)]
    pub pane_id: String,
    /// Whether the pane is focused.
    #[serde(default)]
    pub focused: bool,
    /// Revision.
    #[serde(default)]
    pub revision: u64,
    /// Agent name.
    #[serde(default)]
    pub name: Option<String>,
    /// Completion counter.
    #[serde(default)]
    pub completion_seq: Option<u64>,
    /// State change counter.
    #[serde(default)]
    pub state_change_seq: u64,
}

/// The `session.snapshot` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Server version.
    #[serde(default)]
    pub version: String,
    /// Private protocol number.
    #[serde(default)]
    pub protocol: u32,
    /// Workspaces.
    #[serde(default)]
    pub workspaces: Vec<Workspace>,
    /// Tabs.
    #[serde(default)]
    pub tabs: Vec<Tab>,
    /// Panes.
    #[serde(default)]
    pub panes: Vec<Pane>,
    /// Detected agents.
    #[serde(default)]
    pub agents: Vec<Agent>,
}

/// The `pane.read` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneReadResult {
    /// The read payload.
    pub read: PaneRead,
}

/// The `pane.read` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneRead {
    /// Pane id.
    #[serde(default)]
    pub pane_id: String,
    /// Read source.
    #[serde(default)]
    pub source: String,
    /// `text` or `ansi`.
    #[serde(default)]
    pub format: String,
    /// The text.
    #[serde(default)]
    pub text: String,
    /// Revision marker.
    #[serde(default)]
    pub revision: u64,
    /// Whether the text was truncated.
    #[serde(default)]
    pub truncated: bool,
}

/// The `workspace.create` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceCreated {
    /// The new workspace.
    pub workspace: Workspace,
    /// Its first tab.
    pub tab: Tab,
    /// Its root pane.
    pub root_pane: Pane,
}

/// One subscription line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionFrame {
    /// Event name such as `pane_agent_status_changed`.
    pub event: String,
    /// Event-specific data, including its own `type`.
    #[serde(default)]
    pub data: Value,
}

/// One event payload: `pane_created`, `pane_updated` and friends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneEvent {
    /// Event type.
    #[serde(rename = "type", default)]
    pub kind: String,
    /// The pane, for created and updated events.
    #[serde(default)]
    pub pane: Option<Pane>,
    /// Pane id, for close, exit and status events.
    #[serde(default)]
    pub pane_id: Option<String>,
    /// Workspace id.
    #[serde(default)]
    pub workspace_id: Option<String>,
    /// Agent kind, for detection and status events.
    #[serde(default)]
    pub agent: Option<String>,
    /// Agent status, for status events.
    #[serde(default)]
    pub agent_status: Option<String>,
    /// Final status, for release events.
    #[serde(default)]
    pub final_status: Option<String>,
    /// Whether the agent was released.
    #[serde(default)]
    pub released: bool,
}

/// A frame from `herdr terminal session observe|control`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TerminalStreamFrame {
    /// Terminal output, base64 encoded.
    #[serde(rename = "terminal.frame")]
    Frame {
        /// Frame sequence.
        #[serde(default)]
        seq: u64,
        /// True for a full repaint.
        #[serde(default)]
        full: bool,
        /// Width.
        #[serde(default)]
        width: u16,
        /// Height.
        #[serde(default)]
        height: u16,
        /// Encoding, normally `ansi`.
        #[serde(default)]
        encoding: String,
        /// Base64 payload.
        #[serde(default)]
        bytes: String,
    },
    /// The stream ended.
    #[serde(rename = "terminal.closed")]
    Closed {
        /// Why it ended.
        #[serde(default)]
        reason: Option<String>,
    },
}

/// A command line for the control bridge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type")]
pub enum ControlCommand<'a> {
    /// Typed text.
    #[serde(rename = "terminal.input")]
    Input {
        /// UTF-8 text.
        text: &'a str,
    },
    /// Raw bytes, base64 encoded.
    #[serde(rename = "terminal.input")]
    InputBytes {
        /// Base64 payload.
        bytes: String,
    },
    /// Resize the real pane.
    #[serde(rename = "terminal.resize")]
    Resize {
        /// Columns.
        cols: u16,
        /// Rows.
        rows: u16,
    },
}
