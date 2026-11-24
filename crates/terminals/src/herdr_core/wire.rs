//! Wire types for the herdr-core JSON-RPC API.
//!
//! Only the fields Dandelion needs are declared; serde ignores the rest, so a
//! newer core does not break parsing.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Identity of one core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoreInfo {
    /// Permanent random id.
    pub id: String,
    /// Display name, usually the hostname.
    pub name: String,
}

/// The `hello` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloResult {
    /// Core identity.
    pub core: CoreInfo,
    /// Feature names the core supports.
    #[serde(default)]
    pub features: Vec<String>,
    /// Whether this connection is framed.
    #[serde(default)]
    pub framed: bool,
    /// Protocol generation.
    pub generation: u32,
    /// Method names the core supports.
    #[serde(default)]
    pub methods: Vec<String>,
    /// Core version.
    pub version: String,
}

impl HelloResult {
    /// Returns true when the core advertises a feature.
    #[must_use]
    pub fn has_feature(&self, feature: &str) -> bool {
        self.features.iter().any(|known| known == feature)
    }
}

/// The foreground program of a terminal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Foreground {
    /// Program arguments.
    #[serde(default)]
    pub argv: Vec<String>,
    /// Its working directory.
    #[serde(default)]
    pub cwd: Option<String>,
    /// Program name.
    #[serde(default)]
    pub name: String,
    /// Process id.
    #[serde(default)]
    pub pid: u32,
}

/// One OSC 7501 status record.
///
/// `msg` and `title` can carry conversation text and are deliberately not
/// projected into the common model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Status {
    /// Reporting application, for example `claude-code`.
    #[serde(default)]
    pub app: Option<String>,
    /// Blocked reason kind.
    #[serde(default)]
    pub kind: Option<String>,
    /// Program message (sensitive).
    #[serde(default)]
    pub msg: Option<String>,
    /// Progress in percent.
    #[serde(default)]
    pub progress: Option<u8>,
    /// `idle`, `working`, `done`, `blocked` or `error`.
    pub state: String,
    /// Record title (sensitive).
    #[serde(default)]
    pub title: Option<String>,
}

/// One terminal record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Terminal {
    /// Address such as `t7`.
    pub address: String,
    /// Unique live name.
    #[serde(default)]
    pub name: Option<String>,
    /// Program arguments.
    #[serde(default)]
    pub argv: Vec<String>,
    /// Working directory.
    #[serde(default)]
    pub cwd: String,
    /// Free-form labels.
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
    /// `live` or `finished`.
    #[serde(default)]
    pub state: String,
    /// Creation time in milliseconds since the epoch.
    #[serde(default)]
    pub created_ms: Option<u64>,
    /// Finish time.
    #[serde(default)]
    pub finished_ms: Option<u64>,
    /// Exit code.
    #[serde(default)]
    pub exit_code: Option<i32>,
    /// Ending signal.
    #[serde(default)]
    pub signal: Option<i32>,
    /// Why the program ended.
    #[serde(default)]
    pub reason: Option<String>,
    /// Program process id.
    #[serde(default)]
    pub pid: Option<u32>,
    /// Foreground program, null once finished.
    #[serde(default)]
    pub foreground: Option<Foreground>,
    /// Columns.
    #[serde(default)]
    pub cols: u16,
    /// Rows.
    #[serde(default)]
    pub rows: u16,
    /// Program title.
    #[serde(default)]
    pub title: String,
    /// Root status record.
    #[serde(default)]
    pub status: Option<Status>,
    /// Whether the program ever sent a status.
    #[serde(default)]
    pub reported: bool,
    /// Turns completed.
    #[serde(default)]
    pub turn: u64,
}

/// The `terminal.list` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListResult {
    /// Matching terminals.
    #[serde(default)]
    pub terminals: Vec<Terminal>,
    /// Finished terminals left out.
    #[serde(default)]
    pub hidden_finished: u64,
}

/// The `terminal.read` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadResult {
    /// Plain text, no ANSI.
    #[serde(default)]
    pub text: String,
    /// False when part of what was asked for may be missing.
    #[serde(default)]
    pub complete: bool,
    /// `screen`, `history` or `stitched`.
    #[serde(default)]
    pub how: String,
    /// Why only the screen was returned.
    #[serde(default)]
    pub why: Option<String>,
}

/// The `terminal.send` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendResult {
    /// Whether the program started working.
    #[serde(default)]
    pub started: Option<bool>,
    /// The status word when the text went in.
    #[serde(default)]
    pub status: Option<String>,
    /// Turn when the text went in.
    #[serde(default)]
    pub turn: u64,
}

/// The `terminal.keys` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeysResult {
    /// Status word when the keys went in.
    #[serde(default)]
    pub status: Option<String>,
    /// Turn when the keys went in.
    #[serde(default)]
    pub turn: u64,
}

/// The `terminal.interrupt` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InterruptResult {
    /// The key that was pressed.
    #[serde(default)]
    pub key: String,
    /// `stopped`, `exited`, `still_working` or `unknown`.
    #[serde(default)]
    pub outcome: String,
}

/// The `terminal.rm` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoveResult {
    /// Removed address.
    #[serde(default)]
    pub removed: String,
}

/// One entry of the `events.subscribe` snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscribeResult {
    /// Framed event channel, absent in plain mode.
    #[serde(default)]
    pub channel: Option<u32>,
    /// Events after this sequence follow.
    pub seq: u64,
    /// Matching terminals.
    #[serde(default)]
    pub terminals: Vec<Terminal>,
}

/// The `terminal.attach` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachResult {
    /// Stream channel.
    pub channel: u32,
    /// Whether RESIZE arrives before replacement paints.
    #[serde(default)]
    pub resize_notifications: bool,
    /// Whether the attach path accepts `AUTOMATIC_INPUT`.
    #[serde(default)]
    pub automatic_input: bool,
    /// The terminal as it was at attach time.
    pub terminal: Terminal,
}

/// The payload of a `finished` or `lost` event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinishedData {
    /// Why the program ended.
    #[serde(default)]
    pub reason: Option<String>,
    /// Exit code.
    #[serde(default)]
    pub exit_code: Option<i32>,
    /// Ending signal.
    #[serde(default)]
    pub signal: Option<i32>,
}

/// One `event` notification's parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventParams {
    /// Global sequence.
    pub seq: u64,
    /// The affected terminal; absent on global events such as `doc`.
    #[serde(default)]
    pub terminal: Option<String>,
    /// Event type.
    #[serde(rename = "type")]
    pub kind: String,
    /// Event-specific payload.
    #[serde(default)]
    pub data: serde_json::Value,
}

/// Parameters for `terminal.attach`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AttachParams {
    /// Terminal address.
    pub terminal: String,
    /// Columns.
    pub cols: u16,
    /// Rows.
    pub rows: u16,
    /// Observe only.
    pub observe: bool,
    /// Scrollback lines.
    pub history: u32,
    /// Ask for resize notifications.
    pub resize_notifications: bool,
}
