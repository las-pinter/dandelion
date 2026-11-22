//! What an agent says it is doing.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// What an agent says it is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    /// Waiting for input, with nothing outstanding.
    Idle,
    /// Running a turn.
    Working,
    /// Finished a turn that has not been seen yet.
    Done,
    /// Waiting for a person to answer.
    Blocked,
    /// Failed.
    Error,
    /// The backend could not classify it.
    #[serde(other)]
    Unknown,
}

impl AgentState {
    /// Maps the status word both protocols use.
    #[must_use]
    pub fn from_word(word: &str) -> Self {
        match word {
            "idle" => Self::Idle,
            "working" => Self::Working,
            "done" => Self::Done,
            "blocked" => Self::Blocked,
            "error" => Self::Error,
            _ => Self::Unknown,
        }
    }
}

/// Why a blocked agent is waiting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum BlockedReason {
    /// A permission dialog.
    Permission,
    /// A question for the person.
    Question,
    /// An authentication prompt.
    Auth,
    /// A reason this version does not know.
    #[serde(other)]
    Other,
}

impl BlockedReason {
    /// Maps the herdr-core blocked kind, if the program sent one.
    #[must_use]
    pub fn from_word(word: &str) -> Self {
        match word {
            "permission" => Self::Permission,
            "question" => Self::Question,
            "auth" => Self::Auth,
            _ => Self::Other,
        }
    }
}

/// An agent's state plus the details both protocols can express.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AgentStatus {
    /// The coarse state.
    pub state: AgentState,
    /// Set only while the state is blocked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_reason: Option<BlockedReason>,
    /// Progress in percent, when the program reported it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<u8>,
}

impl AgentStatus {
    /// Builds a status from a state word.
    #[must_use]
    pub fn from_state(state: AgentState) -> Self {
        Self {
            state,
            blocked_reason: None,
            progress: None,
        }
    }

    /// Attaches a blocked reason.
    #[must_use]
    pub fn blocked(reason: BlockedReason) -> Self {
        Self {
            state: AgentState::Blocked,
            blocked_reason: Some(reason),
            progress: None,
        }
    }
}
