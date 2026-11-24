use std::collections::BTreeMap;

use dandelion_claude::SessionStatus;
use dandelion_terminals::{
    AgentKind, AgentState, AgentStatus, BlockedReason, TerminalChange, TerminalId, TerminalInfo,
    TerminalLifecycle,
};

use super::super::Triggers;
use crate::claude::ClaudeAgent;

fn terminal(id: &str, title: &str, lifecycle: TerminalLifecycle) -> TerminalInfo {
    TerminalInfo {
        id: TerminalId::new(id),
        title: title.to_owned(),
        cwd: None,
        labels: BTreeMap::new(),
        agent_kind: Some(AgentKind::normalize("claude-code")),
        agent_state: None,
        lifecycle,
        created_ms: None,
        exit_code: None,
        finished_reason: None,
    }
}

fn status(state: AgentState, reason: Option<BlockedReason>) -> TerminalChange {
    TerminalChange::Status {
        agent_kind: Some(AgentKind::normalize("claude-code")),
        status: AgentStatus {
            state,
            blocked_reason: reason,
            progress: None,
        },
        turn: None,
    }
}

fn session(id: &str, status: SessionStatus, waiting_for: Option<&str>) -> ClaudeAgent {
    ClaudeAgent {
        terminal: None,
        session_id: id.to_owned(),
        cwd: "/home/dev/project".to_owned(),
        name: Some("dandelion-a6".to_owned()),
        agent: None,
        version: None,
        status: Some(status),
        waiting_for: waiting_for.map(str::to_owned),
        updated_at: None,
        usage: dandelion_claude::SessionUsage::empty(id),
    }
}

#[test]
fn a_blocked_agent_is_announced_once_per_transition() {
    let mut triggers = Triggers::default();
    triggers.observe_event(
        "t7",
        &TerminalChange::Upserted {
            terminal: terminal("t7", "claude in dandelion", TerminalLifecycle::Live),
        },
    );

    let first = triggers
        .observe_event(
            "t7",
            &status(AgentState::Blocked, Some(BlockedReason::Permission)),
        )
        .expect("a notice");
    assert_eq!(first.title, "Agent needs you");
    assert_eq!(first.body, "claude in dandelion is waiting for permission.");
    assert_eq!(first.tag, "agent:t7:waiting");

    assert!(
        triggers
            .observe_event(
                "t7",
                &status(AgentState::Blocked, Some(BlockedReason::Permission))
            )
            .is_none(),
        "the same blocked state is not announced twice"
    );

    triggers.observe_event("t7", &status(AgentState::Working, None));
    assert!(
        triggers
            .observe_event("t7", &status(AgentState::Blocked, None))
            .is_some(),
        "a new block after work is announced"
    );
}

#[test]
fn a_finished_agent_is_announced_once() {
    let mut triggers = Triggers::default();
    let finished = TerminalChange::Upserted {
        terminal: terminal("t8", "codex", TerminalLifecycle::Finished),
    };

    let notice = triggers.observe_event("t8", &finished).expect("a notice");
    assert_eq!(notice.tag, "agent:t8:done");
    assert!(triggers.observe_event("t8", &finished).is_none());
}

#[test]
fn a_finished_terminal_without_an_agent_never_notifies() {
    let mut triggers = Triggers::default();
    let mut plain = terminal("t1", "shell", TerminalLifecycle::Finished);
    plain.agent_kind = None;

    assert!(
        triggers
            .observe_event("t1", &TerminalChange::Upserted { terminal: plain })
            .is_none()
    );
}

#[test]
fn a_finished_turn_is_announced_once_per_turn() {
    let mut triggers = Triggers::default();
    triggers.observe_event(
        "t9",
        &TerminalChange::Upserted {
            terminal: terminal("t9", "codex", TerminalLifecycle::Live),
        },
    );

    let done = triggers
        .observe_event(
            "t9",
            &TerminalChange::Turn {
                state: AgentState::Done,
                turn: 3,
            },
        )
        .expect("a finished notice");
    assert_eq!(done.body, "codex finished a turn.");
    assert_eq!(done.tag, "agent:t9:done");

    assert!(
        triggers
            .observe_event(
                "t9",
                &TerminalChange::Turn {
                    state: AgentState::Done,
                    turn: 3,
                },
            )
            .is_none(),
        "a repeated done report is not announced"
    );

    triggers.observe_event(
        "t9",
        &TerminalChange::Turn {
            state: AgentState::Working,
            turn: 4,
        },
    );
    assert!(
        triggers
            .observe_event(
                "t9",
                &TerminalChange::Turn {
                    state: AgentState::Done,
                    turn: 4,
                },
            )
            .is_some(),
        "the next turn is announced"
    );
}

#[test]
fn a_turn_and_a_finish_event_notify() {
    let mut triggers = Triggers::default();
    triggers.observe_event(
        "t9",
        &TerminalChange::Upserted {
            terminal: terminal("t9", "codex", TerminalLifecycle::Live),
        },
    );

    let waiting = triggers
        .observe_event(
            "t9",
            &TerminalChange::Turn {
                state: AgentState::Blocked,
                turn: 3,
            },
        )
        .expect("a waiting notice");
    assert_eq!(waiting.tag, "agent:t9:waiting");

    triggers.observe_event(
        "t9",
        &TerminalChange::Turn {
            state: AgentState::Working,
            turn: 4,
        },
    );
    let done = triggers
        .observe_event(
            "t9",
            &TerminalChange::Finished {
                reason: Some("exited".to_owned()),
                exit_code: Some(0),
                signal: None,
            },
        )
        .expect("a finished notice");
    assert_eq!(done.body, "codex finished (exited).");
    assert_eq!(done.tag, "agent:t9:done");
}

#[test]
fn a_terminal_that_is_not_an_agent_never_notifies() {
    let mut triggers = Triggers::default();
    let mut plain = terminal("t1", "shell", TerminalLifecycle::Live);
    plain.agent_kind = None;
    triggers.observe_event("t1", &TerminalChange::Upserted { terminal: plain });

    assert!(
        triggers
            .observe_event(
                "t1",
                &TerminalChange::Finished {
                    reason: None,
                    exit_code: None,
                    signal: None,
                },
            )
            .is_none()
    );
    let anonymous = TerminalChange::Status {
        agent_kind: None,
        status: AgentStatus {
            state: AgentState::Blocked,
            blocked_reason: Some(BlockedReason::Permission),
            progress: None,
        },
        turn: None,
    };
    assert!(
        triggers.observe_event("t1", &anonymous).is_none(),
        "a status without an agent kind is not an agent"
    );
}

#[test]
fn a_resync_announces_a_waiting_agent_once() {
    let mut triggers = Triggers::default();
    let mut blocked = terminal("t7", "claude in dandelion", TerminalLifecycle::Live);
    blocked.agent_state = Some(AgentStatus {
        state: AgentState::Blocked,
        blocked_reason: Some(BlockedReason::Permission),
        progress: None,
    });

    let notices = triggers.observe_resync(std::slice::from_ref(&blocked));
    assert_eq!(notices.len(), 1);
    assert_eq!(
        notices[0].body,
        "claude in dandelion is waiting for permission."
    );
    assert_eq!(
        triggers
            .observe_resync(std::slice::from_ref(&blocked))
            .len(),
        0,
        "a reconnected snapshot does not repeat the notice"
    );
}

#[test]
fn a_resync_announces_a_blocked_agent_without_a_reason() {
    let mut triggers = Triggers::default();
    let mut blocked = terminal("t7", "claude in dandelion", TerminalLifecycle::Live);
    blocked.agent_state = Some(AgentStatus {
        state: AgentState::Blocked,
        blocked_reason: None,
        progress: None,
    });

    let notices = triggers.observe_resync(std::slice::from_ref(&blocked));
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].body, "claude in dandelion is waiting for you.");
}

#[test]
fn an_unmanaged_waiting_session_is_announced_once() {
    let mut triggers = Triggers::default();
    let waiting = session("s-1", SessionStatus::Waiting, Some("permission prompt"));

    let notices = triggers.observe_claude(std::slice::from_ref(&waiting));
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].body, "dandelion-a6 is waiting for permission.");
    assert_eq!(notices[0].tag, "claude:s-1:waiting");
    assert_eq!(
        triggers
            .observe_claude(std::slice::from_ref(&waiting))
            .len(),
        0,
        "the same waiting session is not announced twice"
    );

    triggers.observe_claude(std::slice::from_ref(&session(
        "s-1",
        SessionStatus::Busy,
        None,
    )));
    assert_eq!(
        triggers
            .observe_claude(std::slice::from_ref(&waiting))
            .len(),
        1,
        "a new wait after work is announced"
    );
}

#[test]
fn a_managed_claude_session_is_left_to_the_backend() {
    let mut triggers = Triggers::default();
    let mut managed = session("s-2", SessionStatus::Waiting, Some("input needed"));
    managed.terminal = Some("t7".to_owned());

    assert_eq!(
        triggers
            .observe_claude(std::slice::from_ref(&managed))
            .len(),
        0
    );
}
