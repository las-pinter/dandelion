//! Projects classic herdr workspaces, tabs and panes into the common model.

use std::collections::BTreeMap;

use crate::error::TerminalError;
use crate::events::TerminalChange;
use crate::model::{
    AgentKind, AgentState, AgentStatus, ScreenText, TerminalId, TerminalInfo, TerminalLifecycle,
};
use crate::util::first_non_empty;

use super::wire::{Agent, Pane, PaneEvent, PaneRead, Snapshot, SubscriptionFrame, Tab, Workspace};

/// Converts a whole snapshot, keeping workspace and tab context as labels.
#[must_use]
pub fn snapshot_terminals(snapshot: &Snapshot) -> Vec<TerminalInfo> {
    let workspaces: BTreeMap<&str, &Workspace> = snapshot
        .workspaces
        .iter()
        .map(|workspace| (workspace.workspace_id.as_str(), workspace))
        .collect();
    let tabs: BTreeMap<&str, &Tab> = snapshot
        .tabs
        .iter()
        .map(|tab| (tab.tab_id.as_str(), tab))
        .collect();
    let agents: BTreeMap<&str, &Agent> = snapshot
        .agents
        .iter()
        .map(|agent| (agent.pane_id.as_str(), agent))
        .collect();
    snapshot
        .panes
        .iter()
        .map(|pane| {
            pane_info(
                pane,
                workspaces.get(pane.workspace_id.as_str()).copied(),
                tabs.get(pane.tab_id.as_str()).copied(),
                agents.get(pane.pane_id.as_str()).copied(),
            )
        })
        .collect()
}

/// Converts one pane. Workspace and tab context is optional because events
/// carry only the pane.
#[must_use]
pub fn pane_info(
    pane: &Pane,
    workspace: Option<&Workspace>,
    tab: Option<&Tab>,
    agent: Option<&Agent>,
) -> TerminalInfo {
    let mut labels = pane.state_labels.clone();
    labels.insert("herdr.workspace_id".to_owned(), pane.workspace_id.clone());
    labels.insert("herdr.tab_id".to_owned(), pane.tab_id.clone());
    labels.insert("herdr.pane_id".to_owned(), pane.pane_id.clone());
    if let Some(label) = workspace.filter(|workspace| !workspace.label.is_empty()) {
        labels.insert("herdr.workspace".to_owned(), label.label.clone());
    }
    if let Some(label) = tab.filter(|tab| !tab.label.is_empty()) {
        labels.insert("herdr.tab".to_owned(), label.label.clone());
    }
    if let Some(label) = pane.label.as_deref().filter(|label| !label.is_empty()) {
        labels.insert("herdr.pane".to_owned(), label.to_owned());
    }
    let agent_name = agent
        .and_then(|agent| agent.name.as_deref())
        .filter(|name| !name.is_empty());
    if let Some(name) = agent_name {
        labels.insert("herdr.agent".to_owned(), name.to_owned());
    }

    let kind_word = pane
        .agent
        .as_deref()
        .or(agent.and_then(|agent| agent.agent.as_deref()))
        .or(pane.display_agent.as_deref())
        .filter(|kind| !kind.trim().is_empty());
    let kind = kind_word.map(AgentKind::normalize);
    let status_word = if pane.agent_status.is_empty() {
        agent.map_or("unknown", |agent| agent.agent_status.as_str())
    } else {
        pane.agent_status.as_str()
    };
    let state = AgentState::from_word(status_word);
    let agent_state =
        (kind.is_some() || state != AgentState::Unknown).then(|| AgentStatus::from_state(state));

    TerminalInfo {
        id: TerminalId::new(pane.pane_id.clone()),
        title: first_non_empty([
            pane.title.as_deref().unwrap_or_default(),
            pane.terminal_title_stripped.as_deref().unwrap_or_default(),
            pane.terminal_title.as_deref().unwrap_or_default(),
            pane.label.as_deref().unwrap_or_default(),
            pane.pane_id.as_str(),
        ]),
        cwd: pane
            .foreground_cwd
            .clone()
            .or_else(|| pane.cwd.clone())
            .filter(|cwd| !cwd.is_empty()),
        labels,
        agent_kind: kind,
        agent_state,
        lifecycle: TerminalLifecycle::Live,
        created_ms: None,
        exit_code: None,
        finished_reason: None,
    }
}

/// Converts a `pane.read` result.
#[must_use]
pub fn screen_text(read: PaneRead) -> ScreenText {
    ScreenText {
        text: read.text,
        complete: !read.truncated,
        origin: crate::model::ReadOrigin::from_classic_source(&read.source),
        truncated: read.truncated,
    }
}

/// What one subscription line means to the common stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClassicNotice {
    /// Structural change: fetch a fresh snapshot.
    Resync,
    /// One pane fact changed.
    Change {
        /// The pane.
        pane: TerminalId,
        /// The new fact.
        change: TerminalChange,
    },
    /// Nothing the common model expresses.
    Ignore,
}

/// Maps one subscription line.
///
/// # Errors
///
/// Returns [`TerminalError::Protocol`] when a pane event has a payload this
/// crate cannot read.
pub fn event_notice(frame: &SubscriptionFrame) -> Result<ClassicNotice, TerminalError> {
    if frame.event.starts_with("workspace")
        || frame.event.starts_with("tab")
        || frame.event == "layout_updated"
    {
        return Ok(ClassicNotice::Resync);
    }
    if !frame.event.starts_with("pane") {
        return Ok(ClassicNotice::Ignore);
    }

    let event: PaneEvent = serde_json::from_value(frame.data.clone())?;
    let pane_id = event
        .pane_id
        .clone()
        .or_else(|| event.pane.as_ref().map(|pane| pane.pane_id.clone()));
    let Some(pane_id) = pane_id else {
        return Ok(ClassicNotice::Ignore);
    };

    let name = frame.event.as_str();
    let change = match name {
        "pane_created" | "pane_updated" | "pane_focused" | "pane_moved" | "pane.agent_detected" => {
            let Some(pane) = event.pane.as_ref() else {
                return Ok(ClassicNotice::Ignore);
            };
            TerminalChange::Upserted {
                terminal: pane_info(pane, None, None, None),
            }
        }
        "pane_closed" => TerminalChange::Removed,
        "pane_exited" => TerminalChange::Finished {
            reason: Some("exited".to_owned()),
            exit_code: None,
            signal: None,
        },
        "pane.agent_status_changed" | "pane_agent_status_changed" => {
            let Some(status) = event.agent_status.as_deref() else {
                return Ok(ClassicNotice::Ignore);
            };
            TerminalChange::Status {
                agent_kind: event
                    .agent
                    .as_deref()
                    .filter(|agent| !agent.is_empty())
                    .map(AgentKind::normalize),
                status: AgentStatus::from_state(AgentState::from_word(status)),
                turn: None,
            }
        }
        "pane_agent_detected" => {
            let Some(status) = event
                .final_status
                .as_deref()
                .or(event.agent_status.as_deref())
            else {
                return Ok(ClassicNotice::Ignore);
            };
            TerminalChange::Status {
                agent_kind: event
                    .agent
                    .as_deref()
                    .filter(|agent| !agent.is_empty())
                    .map(AgentKind::normalize),
                status: AgentStatus::from_state(AgentState::from_word(status)),
                turn: None,
            }
        }
        "pane_output_changed" | "pane.output_matched" => {
            return Ok(ClassicNotice::Ignore);
        }
        _ => return Ok(ClassicNotice::Ignore),
    };
    Ok(ClassicNotice::Change {
        pane: TerminalId::new(pane_id),
        change,
    })
}

#[cfg(test)]
mod tests {
    use super::{ClassicNotice, event_notice, screen_text, snapshot_terminals};
    use crate::events::TerminalChange;
    use crate::model::{AgentState, ReadOrigin, TerminalFilter};
    use crate::test_support::{classic_events, classic_read, classic_snapshot};

    #[test]
    fn snapshot_maps_workspaces_tabs_and_panes() {
        let terminals = snapshot_terminals(&classic_snapshot());
        assert_eq!(terminals.len(), 3);
        assert_eq!(
            terminals.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            ["w1:p1", "w1:p2", "w2:p1"]
        );

        let agent = &terminals[0];
        assert_eq!(agent.title, "agent");
        assert_eq!(agent.cwd.as_deref(), Some("/home/dev/project"));
        assert_eq!(
            agent
                .agent_kind
                .as_ref()
                .map(crate::model::AgentKind::as_str),
            Some("claude-code")
        );
        assert_eq!(
            agent.agent_state.as_ref().map(|state| state.state),
            Some(AgentState::Blocked)
        );
        assert_eq!(
            agent.labels.get("herdr.workspace").map(String::as_str),
            Some("dandelion")
        );
        assert_eq!(
            agent.labels.get("herdr.tab").map(String::as_str),
            Some("main")
        );
        assert_eq!(
            agent.labels.get("herdr.pane").map(String::as_str),
            Some("agent")
        );
        assert_eq!(
            agent.labels.get("herdr.agent").map(String::as_str),
            Some("reviewer")
        );
        assert_eq!(agent.labels.get("branch").map(String::as_str), Some("main"));

        let shell = &terminals[1];
        assert_eq!(shell.title, "bash");
        assert!(shell.agent_kind.is_none());
        assert!(shell.agent_state.is_none());

        let worker = &terminals[2];
        assert_eq!(
            worker
                .agent_kind
                .as_ref()
                .map(crate::model::AgentKind::as_str),
            Some("pi")
        );
        assert_eq!(
            worker.agent_state.as_ref().map(|s| s.state),
            Some(AgentState::Working)
        );

        let filter = TerminalFilter::all().with_label("herdr.workspace_id", "w1");
        assert_eq!(terminals.iter().filter(|t| filter.matches(t)).count(), 2);
    }

    #[test]
    fn read_fixture_reports_its_source() {
        let text = screen_text(classic_read());
        assert!(text.complete);
        assert!(!text.truncated);
        assert_eq!(text.origin, ReadOrigin::Recent);
        assert!(text.text.contains("ready"));
    }

    #[test]
    fn event_fixture_maps_pane_and_structural_events() {
        let events = classic_events();
        let notices: Vec<ClassicNotice> = events
            .iter()
            .map(|frame| event_notice(frame).expect("notice"))
            .collect();

        assert!(matches!(
            &notices[0],
            ClassicNotice::Change {
                pane,
                change: TerminalChange::Upserted { terminal }
            } if pane.as_str() == "w1:p3" && terminal.id.as_str() == "w1:p3"
        ));

        let ClassicNotice::Change { pane, change } = &notices[1] else {
            panic!("expected status change");
        };
        assert_eq!(pane.as_str(), "w1:p1");
        let TerminalChange::Status {
            agent_kind, status, ..
        } = change
        else {
            panic!("expected status");
        };
        assert_eq!(
            agent_kind.as_ref().map(crate::model::AgentKind::as_str),
            Some("claude-code")
        );
        assert_eq!(status.state, AgentState::Working);

        assert!(matches!(
            &notices[2],
            ClassicNotice::Change {
                change: TerminalChange::Upserted { terminal },
                ..
            } if terminal
                .agent_state
                .as_ref()
                .is_some_and(|state| state.state == AgentState::Working)
        ));
        assert!(matches!(
            &notices[3],
            ClassicNotice::Change {
                change: TerminalChange::Removed,
                ..
            }
        ));
        assert_eq!(notices[4], ClassicNotice::Resync);
        assert_eq!(notices[5], ClassicNotice::Resync);
    }

    #[test]
    fn output_events_are_ignored_on_the_public_socket() {
        let frame = crate::herdr::wire::SubscriptionFrame {
            event: "pane.output_matched".to_owned(),
            data: serde_json::json!({
                "type": "pane.output_matched",
                "pane_id": "w1:p1",
                "workspace_id": "w1",
                "matched_line": "x"
            }),
        };
        assert_eq!(event_notice(&frame).expect("notice"), ClassicNotice::Ignore);
    }
}
