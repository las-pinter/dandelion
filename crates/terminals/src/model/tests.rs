//! Model tests.

use super::{
    AgentKind, AgentState, AgentStatus, BlockedReason, KeyName, TerminalFilter, TerminalInfo,
    TerminalLifecycle,
};
use std::collections::BTreeMap;

#[test]
fn agent_kind_normalises_classic_and_core_spellings() {
    assert_eq!(
        AgentKind::normalize("claude"),
        AgentKind::normalize("claude-code")
    );
    assert_eq!(AgentKind::normalize("  CLAUDE  ").as_str(), "claude-code");
    assert!(AgentKind::normalize("claude").is_claude_code());
    assert_eq!(
        AgentKind::normalize("custom-agent").as_str(),
        "custom-agent"
    );
}

#[test]
fn agent_state_parses_every_word() {
    assert_eq!(AgentState::from_word("idle"), AgentState::Idle);
    assert_eq!(AgentState::from_word("working"), AgentState::Working);
    assert_eq!(AgentState::from_word("done"), AgentState::Done);
    assert_eq!(AgentState::from_word("blocked"), AgentState::Blocked);
    assert_eq!(AgentState::from_word("error"), AgentState::Error);
    assert_eq!(AgentState::from_word("wat"), AgentState::Unknown);
}

#[test]
fn serde_round_trips_unknown_state_and_reason() {
    let state: AgentState = serde_json::from_str("\"paused\"").expect("unknown state");
    assert_eq!(state, AgentState::Unknown);
    let reason: BlockedReason = serde_json::from_str("\"captcha\"").expect("unknown reason");
    assert_eq!(reason, BlockedReason::Other);
    assert_eq!(
        serde_json::to_string(&AgentState::Working).expect("state"),
        "\"working\""
    );
}

#[test]
fn key_names_reject_raw_bytes() {
    assert!(KeyName::new("ctrl+c").is_ok());
    assert!(KeyName::new("f5").is_ok());
    assert!(KeyName::new("_").is_err());
    assert!(KeyName::new("").is_err());
    assert!(KeyName::new("two words").is_err());
    assert!(KeyName::new("x".repeat(33)).is_err());
}

fn terminal(id: &str, labels: &[(&str, &str)], lifecycle: TerminalLifecycle) -> TerminalInfo {
    TerminalInfo {
        id: id.into(),
        title: id.to_owned(),
        cwd: None,
        labels: labels
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect::<BTreeMap<_, _>>(),
        agent_kind: None,
        agent_state: None,
        lifecycle,
        created_ms: None,
        exit_code: None,
        finished_reason: None,
    }
}

#[test]
fn filter_requires_every_label_and_live_terminals_by_default() {
    let live = terminal(
        "t1",
        &[("purpose", "dandelion-test"), ("keep", "yes")],
        TerminalLifecycle::Live,
    );
    let finished = terminal(
        "t2",
        &[("purpose", "dandelion-test")],
        TerminalLifecycle::Finished,
    );

    let filter = TerminalFilter::all().with_label("purpose", "dandelion-test");

    assert!(filter.matches(&live));
    assert!(!filter.matches(&finished));

    let strict = filter.clone().with_label("keep", "no");
    assert!(!strict.matches(&live));

    let all = TerminalFilter {
        include_finished: true,
        ..filter
    };
    assert!(all.matches(&finished));
}

#[test]
fn blocked_status_keeps_its_reason() {
    let status = AgentStatus::blocked(BlockedReason::Permission);
    assert_eq!(status.state, AgentState::Blocked);
    assert_eq!(status.blocked_reason, Some(BlockedReason::Permission));
    let json = serde_json::to_value(&status).expect("status json");
    assert_eq!(json["state"], "blocked");
    assert_eq!(json["blocked_reason"], "permission");
}
