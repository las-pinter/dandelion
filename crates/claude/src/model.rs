//! The public session model and the raw on-disk record it is built from.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use utoipa::openapi::RefOr;
use utoipa::openapi::schema::{ObjectBuilder, Schema, Type};
use utoipa::{PartialSchema, ToSchema};

/// The state a Claude Code session reports for itself.
///
/// The session registry is an internal, evolving format. A value this crate
/// does not recognize is kept verbatim in [`SessionStatus::Other`] so callers
/// and the API surface never lose information.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SessionStatus {
    /// The agent is working.
    Busy,
    /// The agent is idle.
    Idle,
    /// The agent is paused while a shell command runs.
    Shell,
    /// The agent is blocked on a prompt; see [`Session::waiting_for`].
    Waiting,
    /// A value this version does not know, preserved exactly as written.
    Other(String),
}

impl SessionStatus {
    /// Parses a status string, preserving unknown values.
    #[must_use]
    pub fn from_raw(raw: &str) -> Self {
        match raw {
            "busy" => Self::Busy,
            "idle" => Self::Idle,
            "shell" => Self::Shell,
            "waiting" => Self::Waiting,
            other => Self::Other(other.to_owned()),
        }
    }

    /// The wire representation of this status.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Busy => "busy",
            Self::Idle => "idle",
            Self::Shell => "shell",
            Self::Waiting => "waiting",
            Self::Other(raw) => raw,
        }
    }

    /// Whether the session is blocked on a prompt.
    #[must_use]
    pub const fn is_waiting(&self) -> bool {
        matches!(self, Self::Waiting)
    }
}

impl Serialize for SessionStatus {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for SessionStatus {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ok(Self::from_raw(&raw))
    }
}

impl PartialSchema for SessionStatus {
    fn schema() -> RefOr<Schema> {
        ObjectBuilder::new()
            .schema_type(Type::String)
            .description(Some(
                "Session state. Unknown values are preserved verbatim rather than mapped.",
            ))
            .into()
    }
}

impl ToSchema for SessionStatus {
    fn schemas(schemas: &mut Vec<(String, RefOr<Schema>)>) {
        schemas.push((Self::name().into(), Self::schema()));
    }
}

/// A live Claude Code session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    /// Process id of the `claude` process.
    pub pid: u32,
    /// Session id, used to locate the transcript.
    pub session_id: String,
    /// Working directory the session was launched in.
    pub cwd: String,
    /// Human-readable session name, when set.
    pub name: Option<String>,
    /// Agent name when the session runs an agent.
    pub agent: Option<String>,
    /// Claude Code version reported by the session.
    pub version: Option<String>,
    /// Current state, when the record carries one.
    pub status: Option<SessionStatus>,
    /// What a waiting session is waiting for, when reported.
    pub waiting_for: Option<String>,
    /// herdr-core terminal this session runs in, from `HC_TERMINAL_ID`.
    pub terminal_id: Option<String>,
    /// Start time in Unix milliseconds.
    pub started_at: Option<i64>,
    /// Last update time in Unix milliseconds.
    pub updated_at: Option<i64>,
}

/// One `~/.claude/sessions/<pid>.json` file.
///
/// Only the fields this crate needs are read. Unknown fields, including
/// anything that could hold conversation content, are ignored.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SessionRecord {
    pub pid: u32,
    pub session_id: String,
    pub cwd: String,
    pub proc_start: Option<String>,
    pub version: Option<String>,
    pub name: Option<String>,
    pub agent: Option<String>,
    pub status: Option<String>,
    pub waiting_for: Option<String>,
    pub started_at: Option<i64>,
    pub updated_at: Option<i64>,
}

impl SessionRecord {
    pub fn into_session(self, terminal_id: Option<String>) -> Session {
        Session {
            pid: self.pid,
            session_id: self.session_id,
            cwd: self.cwd,
            name: self.name,
            agent: self.agent,
            version: self.version,
            status: self.status.as_deref().map(SessionStatus::from_raw),
            waiting_for: self.waiting_for,
            terminal_id,
            started_at: self.started_at,
            updated_at: self.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SessionStatus;

    #[test]
    fn parses_known_statuses() {
        assert_eq!(SessionStatus::from_raw("busy"), SessionStatus::Busy);
        assert_eq!(SessionStatus::from_raw("idle"), SessionStatus::Idle);
        assert_eq!(SessionStatus::from_raw("shell"), SessionStatus::Shell);
        assert_eq!(SessionStatus::from_raw("waiting"), SessionStatus::Waiting);
    }

    #[test]
    fn preserves_unknown_status() {
        let status = SessionStatus::from_raw("hibernating");

        assert_eq!(status, SessionStatus::Other("hibernating".to_owned()));
        assert_eq!(status.as_str(), "hibernating");
    }

    #[test]
    fn serializes_as_a_plain_string() {
        let status = SessionStatus::Waiting;

        let json = serde_json::to_string(&status).unwrap_or_default();

        assert_eq!(json, "\"waiting\"");
    }

    #[test]
    fn deserializes_unknown_value_verbatim() {
        let status: SessionStatus = serde_json::from_str("\"paused-for-coffee\"")
            .unwrap_or_else(|error| panic!("expected a status string, got {error}"));

        assert_eq!(status, SessionStatus::Other("paused-for-coffee".to_owned()));
    }
}
