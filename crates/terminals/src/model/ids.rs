//! Identifiers, agent kinds and key names.

use std::fmt;

use serde::{Deserialize, Serialize};
use utoipa::{PartialSchema, ToSchema};

use crate::error::TerminalError;

/// Gives a string newtype a plain string `OpenAPI` schema.
macro_rules! string_newtype_schema {
    ($ty:ty) => {
        impl PartialSchema for $ty {
            fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
                <String as PartialSchema>::schema()
            }
        }

        impl ToSchema for $ty {
            fn name() -> std::borrow::Cow<'static, str> {
                std::borrow::Cow::Borrowed(stringify!($ty))
            }
        }
    };
}

/// Identifies a terminal inside one backend.
///
/// herdr-core uses addresses such as `t7` or `t1b`; classic herdr uses pane ids
/// such as `w1:p2`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TerminalId(String);

impl TerminalId {
    /// Creates a terminal id from any string the backend returned.
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the id as the backend spells it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TerminalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for TerminalId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for TerminalId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

string_newtype_schema!(TerminalId);

/// The kind of coding agent a terminal runs.
///
/// Kind names are open-ended in both protocols, so this stays a string in
/// disguise. Known spellings are normalised: classic herdr says `claude`, while
/// herdr-core agents report `claude-code`; both become `claude-code`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AgentKind(String);

impl AgentKind {
    /// Claude Code.
    pub const CLAUDE_CODE: &'static str = "claude-code";
    /// `OpenAI` Codex.
    pub const CODEX: &'static str = "codex";
    /// Google Gemini.
    pub const GEMINI: &'static str = "gemini";
    /// Pi.
    pub const PI: &'static str = "pi";
    /// Cursor.
    pub const CURSOR: &'static str = "cursor";
    /// Cline.
    pub const CLINE: &'static str = "cline";
    /// opencode.
    pub const OPENCODE: &'static str = "opencode";
    /// GitHub Copilot.
    pub const COPILOT: &'static str = "copilot";

    /// Normalises a backend's agent name into the common spelling.
    #[must_use]
    pub fn normalize(raw: &str) -> Self {
        let trimmed = raw.trim();
        let canonical = match trimmed.to_ascii_lowercase().as_str() {
            "claude" | "claude-code" | "claude_code" | "claudecode" => Self::CLAUDE_CODE,
            "codex" => Self::CODEX,
            "gemini" => Self::GEMINI,
            "pi" => Self::PI,
            "cursor" => Self::CURSOR,
            "cline" => Self::CLINE,
            "opencode" => Self::OPENCODE,
            "copilot" => Self::COPILOT,
            _ => trimmed,
        };
        Self(canonical.to_owned())
    }

    /// Returns the normalised kind.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns true for Claude Code.
    #[must_use]
    pub fn is_claude_code(&self) -> bool {
        self.0 == Self::CLAUDE_CODE
    }
}

impl fmt::Display for AgentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

string_newtype_schema!(AgentKind);

/// A key name understood by both backends (`enter`, `ctrl+c`, `f5`, ...).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct KeyName(String);

impl KeyName {
    /// Validates and creates a key name.
    ///
    /// # Errors
    ///
    /// Returns [`TerminalError::InvalidKey`] when the name is empty, longer
    /// than 32 bytes, or contains whitespace or control characters.
    pub fn new(name: impl Into<String>) -> Result<Self, TerminalError> {
        let name = name.into();
        let valid = !name.is_empty()
            && name.len() <= 32
            && name != "_"
            && name
                .bytes()
                .all(|b| b.is_ascii_graphic() && !b.is_ascii_whitespace());
        if valid {
            Ok(Self(name))
        } else {
            Err(TerminalError::InvalidKey(name))
        }
    }

    /// The Enter key.
    #[must_use]
    pub fn enter() -> Self {
        Self("enter".to_owned())
    }

    /// The Escape key.
    #[must_use]
    pub fn escape() -> Self {
        Self("esc".to_owned())
    }

    /// Ctrl+C.
    #[must_use]
    pub fn ctrl_c() -> Self {
        Self("ctrl+c".to_owned())
    }

    /// Returns the key name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for KeyName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

string_newtype_schema!(KeyName);
