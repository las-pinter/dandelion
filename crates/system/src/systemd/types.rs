//! Serializable types shared by the systemd service and its callers.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Which manager owns a unit: the user manager or the system manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum UnitScope {
    /// The manager for this login session (`systemctl --user`).
    User,
    /// The machine-wide manager (`systemctl`).
    System,
}

/// A unit as listed by a manager.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UnitInfo {
    /// Full unit name, such as `nginx.service`.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Load state, such as `loaded` or `not-found`.
    pub load_state: String,
    /// Active state, such as `active` or `failed`.
    pub active_state: String,
    /// Sub state, such as `running` or `exited`.
    pub sub_state: String,
    /// The manager that reported the unit.
    pub scope: UnitScope,
}

/// Detailed status of one unit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UnitStatus {
    /// Full unit name.
    pub name: String,
    /// The manager that reported the unit.
    pub scope: UnitScope,
    /// Human-readable description.
    pub description: String,
    /// Load state, such as `loaded` or `not-found`.
    pub load_state: String,
    /// Active state, such as `active` or `failed`.
    pub active_state: String,
    /// Sub state, such as `running` or `exited`.
    pub sub_state: String,
    /// Enabling state, such as `enabled`, `disabled`, or `transient` for a
    /// unit created by `systemd-run`.
    pub unit_file_state: Option<String>,
    /// Path of the unit file. A transient unit's fragment lives under the
    /// manager's transient directory.
    pub fragment_path: Option<String>,
}

/// A mutating action on a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum UnitAction {
    /// Start the unit.
    Start,
    /// Stop the unit.
    Stop,
    /// Restart the unit.
    Restart,
    /// Enable the unit at boot or login.
    Enable,
    /// Disable the unit at boot or login.
    Disable,
}

impl UnitAction {
    /// The verb accepted by `systemctl`.
    #[must_use]
    pub const fn systemctl_verb(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
            Self::Enable => "enable",
            Self::Disable => "disable",
        }
    }
}

impl std::fmt::Display for UnitAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.systemctl_verb())
    }
}

/// A typed request for a mutating unit action. The server audits this value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UnitActionRequest {
    /// The manager that owns the unit.
    pub scope: UnitScope,
    /// Full unit name. System scope also requires an allowlist entry.
    pub unit: String,
    /// The action to perform.
    pub action: UnitAction,
}

/// How an action reached systemd.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActionMechanism {
    /// A direct D-Bus call to the user manager.
    Dbus,
    /// `sudo -n systemctl` after an allowlist check.
    Sudo,
}

/// What a mutating unit action did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UnitActionResult {
    /// The manager that owned the unit.
    pub scope: UnitScope,
    /// The unit name.
    pub unit: String,
    /// The action that ran.
    pub action: UnitAction,
    /// The path the action took.
    pub mechanism: ActionMechanism,
}

/// A request to read the last lines of a unit's journal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct JournalTailRequest {
    /// The manager that owns the unit.
    pub scope: UnitScope,
    /// Full unit name.
    pub unit: String,
    /// Number of most recent entries to return.
    pub lines: usize,
}

/// A request to follow a unit's journal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct JournalFollowRequest {
    /// The manager that owns the unit.
    pub scope: UnitScope,
    /// Full unit name.
    pub unit: String,
    /// Number of entries to replay before following. `0` follows only new
    /// entries; `None` uses journalctl's default.
    pub lines: Option<usize>,
}

/// One journal record, reduced to the fields the UI shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct JournalEntry {
    /// Record time as microseconds since the Unix epoch.
    pub timestamp_usec: Option<i64>,
    /// Log message, with invalid UTF-8 replaced.
    pub message: String,
    /// Syslog severity, 0 (emergency) to 7 (debug).
    pub priority: Option<u8>,
    /// The unit that wrote the record, when it has one.
    pub unit: Option<String>,
    /// Syslog identifier, or the command name when it has none.
    pub identifier: Option<String>,
    /// Process id of the writer, when known.
    pub pid: Option<u32>,
}
