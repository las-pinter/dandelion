//! Filters used by listing and subscribing.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{TerminalId, TerminalInfo};

/// Filters used by listing and subscribing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TerminalFilter {
    /// Every label must match (AND semantics).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,
    /// Include terminals whose program has ended.
    #[serde(default)]
    pub include_finished: bool,
    /// Restrict to one terminal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal: Option<TerminalId>,
}

impl TerminalFilter {
    /// Matches every terminal.
    #[must_use]
    pub fn all() -> Self {
        Self::default()
    }

    /// Restricts to one terminal.
    #[must_use]
    pub fn one(id: TerminalId) -> Self {
        Self {
            terminal: Some(id),
            ..Self::default()
        }
    }

    /// Requires one label.
    #[must_use]
    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }

    /// Returns true when `info` passes this filter.
    #[must_use]
    pub fn matches(&self, info: &TerminalInfo) -> bool {
        if !self.include_finished && !info.is_live() {
            return false;
        }
        if self.terminal.as_ref().is_some_and(|id| id != &info.id) {
            return false;
        }
        self.labels
            .iter()
            .all(|(key, value)| info.labels.get(key).is_some_and(|found| found == value))
    }
}
