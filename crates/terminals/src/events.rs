//! Common change events emitted by backend subscriptions.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::model::{AgentKind, AgentState, AgentStatus, TerminalId, TerminalInfo, TerminalSize};

/// A full snapshot a consumer must apply before later events.
///
/// A backend sends one at subscription start, and again after it reports that
/// the subscriber missed events. Ignore terminal events whose `seq` is not
/// greater than the snapshot's `seq`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Resync {
    /// The sequence the snapshot covers.
    pub seq: u64,
    /// Every terminal matching the subscription.
    pub terminals: Vec<TerminalInfo>,
}

/// One changed fact about one terminal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TerminalEvent {
    /// Globally ordered within one subscription.
    pub seq: u64,
    /// The terminal the fact belongs to.
    pub terminal: TerminalId,
    /// What changed.
    pub change: TerminalChange,
}

/// A fact change both backends can express.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TerminalChange {
    /// The terminal appeared, or its full record was refreshed.
    Upserted {
        /// The terminal as it now is.
        terminal: TerminalInfo,
    },
    /// The terminal was removed from the backend.
    Removed,
    /// The agent's own status changed.
    Status {
        /// The normalised agent kind, when known.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent_kind: Option<AgentKind>,
        /// The new status.
        status: AgentStatus,
        /// The turn the status belongs to, when reported.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        turn: Option<u64>,
    },
    /// The agent left the working state.
    Turn {
        /// The state it moved to.
        state: AgentState,
        /// The new turn number.
        turn: u64,
    },
    /// Labels changed.
    Labels {
        /// The full label set after the change.
        labels: BTreeMap<String, String>,
    },
    /// The terminal was resized.
    Size {
        /// The new size.
        size: TerminalSize,
    },
    /// The title changed.
    Title {
        /// The new title.
        title: String,
    },
    /// The foreground program changed.
    Foreground {
        /// Program name.
        name: String,
        /// Program arguments.
        argv: Vec<String>,
        /// Process id, when reported.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pid: Option<u32>,
    },
    /// The program ended, but the terminal may still be readable.
    Finished {
        /// Why it ended.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
        /// Its exit code, when it had one.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exit_code: Option<i32>,
        /// The signal that ended it, when one did.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signal: Option<i32>,
    },
}

/// What a subscription delivers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BackendEvent {
    /// A full snapshot, first or after missed events.
    Resync(Resync),
    /// One fact change.
    Terminal(TerminalEvent),
    /// The backend is alive; its sequence is informational.
    Heartbeat {
        /// The backend's current sequence.
        seq: u64,
    },
}

impl BackendEvent {
    /// Returns the event's sequence, when it has one.
    #[must_use]
    pub fn seq(&self) -> Option<u64> {
        match self {
            Self::Resync(resync) => Some(resync.seq),
            Self::Terminal(event) => Some(event.seq),
            Self::Heartbeat { seq } => Some(*seq),
        }
    }
}

/// Drops events a resync already covers.
#[derive(Debug, Clone, Copy, Default)]
pub struct EventGate {
    floor: u64,
}

impl EventGate {
    /// A gate that accepts every sequence.
    #[must_use]
    pub const fn new() -> Self {
        Self { floor: 0 }
    }

    /// Applies a snapshot sequence, raising the floor.
    pub fn resync(&mut self, seq: u64) {
        self.floor = self.floor.max(seq);
    }

    /// Returns true when an event with this sequence follows the snapshot.
    #[must_use]
    pub const fn accepts(&self, seq: u64) -> bool {
        seq > self.floor
    }
}

#[cfg(test)]
mod tests {
    use super::EventGate;

    #[test]
    fn gate_drops_events_covered_by_a_resync() {
        let mut gate = EventGate::new();
        assert!(gate.accepts(1));

        gate.resync(40);

        assert!(!gate.accepts(40));
        assert!(!gate.accepts(39));
        assert!(gate.accepts(41));
        gate.resync(10);
        assert!(gate.accepts(41));
    }
}
