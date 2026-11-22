//! Everything needed to start a terminal.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::TerminalSize;

/// Everything needed to start a terminal.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SpawnSpec {
    /// Program and arguments. Ignored when `line` is set.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub argv: Vec<String>,
    /// A shell line to run instead of `argv`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
    /// A unique live-terminal name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Working directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Labels.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, String>,
    /// Extra environment variables.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    /// Initial size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<TerminalSize>,
    /// Restart a lost program as a shell in place.
    pub keep: bool,
    /// Do not make the caller the parent (herdr-core).
    pub no_parent: bool,
    /// Do not type notes into the parent (herdr-core).
    pub no_notify: bool,
}

impl SpawnSpec {
    /// An empty spec.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets `argv`.
    #[must_use]
    pub fn argv(mut self, argv: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.argv = argv.into_iter().map(Into::into).collect();
        self
    }

    /// Sets a shell line.
    #[must_use]
    pub fn line(mut self, line: impl Into<String>) -> Self {
        self.line = Some(line.into());
        self
    }

    /// Sets a label.
    #[must_use]
    pub fn label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }

    /// Sets the name.
    #[must_use]
    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Sets the initial size.
    #[must_use]
    pub fn sized(mut self, size: TerminalSize) -> Self {
        self.size = Some(size);
        self
    }

    /// Detaches from the caller and silences parent notes.
    #[must_use]
    pub fn detached(mut self) -> Self {
        self.no_parent = true;
        self.no_notify = true;
        self
    }
}
