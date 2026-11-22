//! Read, send and attach requests and their results.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{AgentState, KeyName, TerminalSize};

/// Where the text of a screen read came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReadOrigin {
    /// The current screen.
    Screen,
    /// The visible viewport of a classic herdr pane.
    Visible,
    /// Recent scrollback.
    Recent,
    /// Recent scrollback with wrapped lines joined.
    RecentUnwrapped,
    /// The agent detector's snapshot.
    Detection,
    /// History stitched by scrolling a full-screen program.
    Stitched,
    /// An origin this version does not know.
    #[serde(other)]
    Unknown,
}

impl ReadOrigin {
    /// Maps herdr-core's `read.how`.
    #[must_use]
    pub fn from_core_how(how: &str) -> Self {
        match how {
            "screen" => Self::Screen,
            "history" => Self::Recent,
            "stitched" => Self::Stitched,
            _ => Self::Unknown,
        }
    }

    /// Maps classic herdr's `read.source`.
    #[must_use]
    pub fn from_classic_source(source: &str) -> Self {
        match source {
            "visible" => Self::Visible,
            "recent" => Self::Recent,
            "recent_unwrapped" => Self::RecentUnwrapped,
            "detection" => Self::Detection,
            _ => Self::Unknown,
        }
    }
}

/// Plain text read from a terminal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ScreenText {
    /// The text.
    pub text: String,
    /// False when part of what was asked for may be missing.
    pub complete: bool,
    /// Where the text came from.
    pub origin: ReadOrigin,
    /// True when the backend truncated the text.
    pub truncated: bool,
}

/// How a send request ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SendOutcome {
    /// Whether the agent went to work; `None` when it never reports status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started: Option<bool>,
    /// The agent's word when the text went in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<AgentState>,
    /// The turn when the text went in.
    pub turn: u64,
}

/// What an interrupt did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum InterruptResult {
    /// The program left its working state, or a shell got its prompt back.
    Stopped,
    /// The program exited.
    Exited,
    /// Nothing changed within the timeout.
    StillWorking,
    /// The backend cannot say (classic herdr).
    #[serde(other)]
    Unknown,
}

impl InterruptResult {
    /// Maps herdr-core's `interrupt.outcome`.
    #[must_use]
    pub fn from_outcome(outcome: &str) -> Self {
        match outcome {
            "stopped" => Self::Stopped,
            "exited" => Self::Exited,
            "still_working" => Self::StillWorking,
            _ => Self::Unknown,
        }
    }
}

/// The result of an interrupt request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct InterruptOutcome {
    /// The key that was pressed.
    pub key: KeyName,
    /// What the backend observed.
    pub result: InterruptResult,
}

/// Options for reading a terminal screen.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ReadOptions {
    /// History lines above the screen. Keep this at zero for terminals Dandelion
    /// does not own: herdr-core scrolls a full-screen program to stitch history.
    pub lines: u32,
    /// Ask classic herdr for ANSI colours. herdr-core always returns plain text.
    pub ansi: bool,
}

impl ReadOptions {
    /// The safe read for a terminal Dandelion did not create: screen only.
    #[must_use]
    pub const fn screen() -> Self {
        Self {
            lines: 0,
            ansi: false,
        }
    }

    /// A read with history. Only use this on terminals Dandelion owns.
    #[must_use]
    pub const fn history(lines: u32) -> Self {
        Self { lines, ansi: false }
    }
}

/// Options for sending text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SendOptions {
    /// Send even when the program is blocked.
    pub force: bool,
    /// Only send when the agent is in this state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub only_if: Option<AgentState>,
    /// Ask the backend to wait for the turn to end.
    pub wait_for_settle: bool,
    /// Override the backend's wait timeout, in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

/// How to attach to a terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AttachMode {
    /// Never type or resize. Use this for every terminal Dandelion did not create.
    Observe,
    /// Accept input and resize the real terminal.
    Control,
}

/// Options for an attach stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct AttachOptions {
    /// Observe or control.
    pub mode: AttachMode,
    /// Viewport size sent to the backend.
    pub size: TerminalSize,
    /// Scrollback lines to prepend.
    pub history: u32,
}

impl Default for AttachOptions {
    fn default() -> Self {
        Self {
            mode: AttachMode::Observe,
            size: TerminalSize::DEFAULT,
            history: 0,
        }
    }
}
