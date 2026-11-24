//! Turns state transitions into notices, once per transition.
//!
//! The state lives in memory: a restart may repeat a notice that is still
//! true, which is preferable to losing one. Agent transitions, resource
//! thresholds, and unit failures each keep their own part of that memory.

mod agents;
mod resources;
mod units;

use std::collections::{HashMap, HashSet};

pub use resources::Resource;

/// How an agent was last seen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Alive and not waiting on a person.
    Live,
    /// Waiting on a person.
    Blocked,
    /// Finished a turn, or exited. One event per crossing.
    Done,
}

/// The transition memory behind the watchers.
#[derive(Debug, Default)]
pub struct Triggers {
    phases: HashMap<String, Phase>,
    labels: HashMap<String, String>,
    /// Terminals that report an agent kind; other terminals never notify.
    agents: HashSet<String>,
    above_since: HashMap<Resource, u64>,
    high_notified: HashSet<Resource>,
    failed_units: HashSet<String>,
    /// Claude session ids that were waiting on the previous pass.
    waiting_sessions: HashSet<String>,
}

impl Triggers {
    fn phase(&self, terminal: &str) -> Phase {
        self.phases.get(terminal).copied().unwrap_or(Phase::Live)
    }

    fn set_phase(&mut self, terminal: &str, phase: Phase) {
        self.phases.insert(terminal.to_owned(), phase);
    }

    fn label(&self, terminal: &str) -> String {
        self.labels
            .get(terminal)
            .cloned()
            .unwrap_or_else(|| terminal.to_owned())
    }
}
