//! Agent transitions: blocked, finished a turn, exited, and the Claude
//! sessions started outside herdr.

use dandelion_claude::SessionStatus;
use dandelion_terminals::{
    AgentState, BlockedReason, TerminalChange, TerminalInfo, TerminalLifecycle,
};

use super::{Phase, Triggers};
use crate::claude::ClaudeAgent;
use crate::notifications::Notice;

impl Triggers {
    /// Records a backend event and returns the notice it deserves, if any.
    pub fn observe_event(&mut self, terminal: &str, change: &TerminalChange) -> Option<Notice> {
        match change {
            TerminalChange::Upserted { terminal: info } => {
                let label = label_of(info);
                self.labels.insert(terminal.to_owned(), label.clone());
                if info.agent_kind.is_some() {
                    self.agents.insert(terminal.to_owned());
                } else {
                    self.agents.remove(terminal);
                }
                if !self.agents.contains(terminal) {
                    return None;
                }
                match info.lifecycle {
                    TerminalLifecycle::Finished => self.finish(terminal, &label, None),
                    TerminalLifecycle::Live => {
                        match info.agent_state.as_ref().map(|status| status.state) {
                            Some(AgentState::Blocked) => {
                                let reason = info
                                    .agent_state
                                    .as_ref()
                                    .and_then(|status| status.blocked_reason.as_ref());
                                self.block(terminal, &label, reason)
                            }
                            Some(AgentState::Done) => self.done(terminal, &label),
                            _ => {
                                self.set_phase(terminal, Phase::Live);
                                None
                            }
                        }
                    }
                }
            }
            TerminalChange::Status {
                agent_kind, status, ..
            } => {
                if agent_kind.is_some() {
                    self.agents.insert(terminal.to_owned());
                }
                if !self.agents.contains(terminal) {
                    return None;
                }
                let label = self.label(terminal);
                match status.state {
                    AgentState::Blocked => {
                        self.block(terminal, &label, status.blocked_reason.as_ref())
                    }
                    AgentState::Done => self.done(terminal, &label),
                    _ => {
                        self.set_phase(terminal, Phase::Live);
                        None
                    }
                }
            }
            TerminalChange::Turn { state, .. } => {
                if !self.agents.contains(terminal) {
                    return None;
                }
                let label = self.label(terminal);
                match state {
                    AgentState::Blocked => self.block(terminal, &label, None),
                    AgentState::Done => self.done(terminal, &label),
                    _ => {
                        self.set_phase(terminal, Phase::Live);
                        None
                    }
                }
            }
            TerminalChange::Finished { reason, .. } => {
                if !self.agents.contains(terminal) {
                    return None;
                }
                let label = self.label(terminal);
                self.finish(terminal, &label, reason.as_deref())
            }
            TerminalChange::Removed => {
                self.phases.remove(terminal);
                self.labels.remove(terminal);
                self.agents.remove(terminal);
                None
            }
            _ => None,
        }
    }

    /// Rebuilds the known terminals from a resync and announces agents that
    /// are already waiting, which a restart would otherwise swallow.
    pub fn observe_resync(&mut self, terminals: &[TerminalInfo]) -> Vec<Notice> {
        // A reconnect keeps the transition memory: a still-blocked agent is
        // announced on the first snapshot only.
        let previous = std::mem::take(&mut self.phases);
        self.labels.clear();
        self.agents.clear();
        let mut notices = Vec::new();
        for terminal in terminals {
            let id = terminal.id.to_string();
            let label = label_of(terminal);
            self.labels.insert(id.clone(), label);
            let is_agent = terminal.agent_kind.is_some();
            if is_agent {
                self.agents.insert(id.clone());
            }
            let blocked = terminal
                .agent_state
                .as_ref()
                .filter(|status| status.state == AgentState::Blocked);
            let blocked_reason = blocked.and_then(|status| status.blocked_reason.as_ref());
            let phase = if terminal.lifecycle == TerminalLifecycle::Finished {
                Phase::Done
            } else if blocked.is_some() {
                Phase::Blocked
            } else {
                Phase::Live
            };
            self.set_phase(&id, phase);
            if is_agent && phase == Phase::Blocked && previous.get(&id) != Some(&Phase::Blocked) {
                notices.push(waiting_notice(&id, &self.label(&id), blocked_reason));
            }
        }
        notices
    }

    /// Diffs Claude Code's sessions and announces unmanaged ones that started
    /// waiting. Sessions inside a herdr terminal are already announced by the
    /// backend's own status events.
    pub fn observe_claude(&mut self, sessions: &[ClaudeAgent]) -> Vec<Notice> {
        let waiting: Vec<&ClaudeAgent> = sessions
            .iter()
            .filter(|session| session.terminal.is_none())
            .filter(|session| {
                session
                    .status
                    .as_ref()
                    .is_some_and(SessionStatus::is_waiting)
            })
            .collect();
        let mut notices = Vec::new();
        for session in &waiting {
            if !self.waiting_sessions.contains(&session.session_id) {
                let label = claude_label(session);
                notices.push(Notice::new(
                    "Agent needs you",
                    format!(
                        "{label} is waiting for {}.",
                        claude_phrase(session.waiting_for.as_deref())
                    ),
                    format!("claude:{}:waiting", session.session_id),
                    "/agents",
                ));
            }
        }
        self.waiting_sessions = waiting
            .iter()
            .map(|session| session.session_id.clone())
            .collect();
        notices
    }

    fn block(
        &mut self,
        terminal: &str,
        label: &str,
        reason: Option<&BlockedReason>,
    ) -> Option<Notice> {
        if self.phase(terminal) == Phase::Blocked {
            return None;
        }
        self.set_phase(terminal, Phase::Blocked);
        Some(waiting_notice(terminal, label, reason))
    }

    /// Announces a completed turn once, however many `done` reports follow.
    fn done(&mut self, terminal: &str, label: &str) -> Option<Notice> {
        if self.phase(terminal) == Phase::Done {
            return None;
        }
        self.set_phase(terminal, Phase::Done);
        Some(Notice::new(
            "Agent finished",
            format!("{label} finished a turn."),
            format!("agent:{terminal}:done"),
            "/agents",
        ))
    }

    fn finish(&mut self, terminal: &str, label: &str, reason: Option<&str>) -> Option<Notice> {
        if self.phase(terminal) == Phase::Done {
            return None;
        }
        self.set_phase(terminal, Phase::Done);
        let body = match reason {
            Some(reason) => format!("{label} finished ({reason})."),
            None => format!("{label} finished."),
        };
        Some(Notice::new(
            "Agent finished",
            body,
            format!("agent:{terminal}:done"),
            "/agents",
        ))
    }
}

fn waiting_notice(terminal: &str, label: &str, reason: Option<&BlockedReason>) -> Notice {
    Notice::new(
        "Agent needs you",
        format!("{label} is waiting for {}.", blocked_phrase(reason)),
        format!("agent:{terminal}:waiting"),
        "/agents",
    )
}

/// A terminal title is free text; a notice body must stay well inside the
/// push payload limit.
const MAX_LABEL_CHARS: usize = 80;

fn label_of(terminal: &TerminalInfo) -> String {
    if terminal.title.is_empty() {
        terminal.id.to_string()
    } else {
        truncate(terminal.title.clone())
    }
}

/// The name of an unmanaged session: its own name, its agent, or a short id.
fn claude_label(session: &ClaudeAgent) -> String {
    truncate(
        session
            .name
            .clone()
            .or_else(|| session.agent.clone())
            .unwrap_or_else(|| session.session_id.chars().take(8).collect()),
    )
}

fn truncate(label: String) -> String {
    if label.chars().count() <= MAX_LABEL_CHARS {
        return label;
    }
    let mut short: String = label.chars().take(MAX_LABEL_CHARS).collect();
    short.push('…');
    short
}

fn blocked_phrase(reason: Option<&BlockedReason>) -> &'static str {
    match reason {
        Some(BlockedReason::Permission) => "permission",
        Some(BlockedReason::Question) => "an answer",
        Some(BlockedReason::Auth) => "sign-in",
        Some(BlockedReason::Other) | None => "you",
    }
}

fn claude_phrase(waiting_for: Option<&str>) -> &'static str {
    match waiting_for {
        Some(text) if text.contains("permission") => "permission",
        Some(text) if text.contains("input") => "an answer",
        _ => "you",
    }
}

#[cfg(test)]
mod tests;
