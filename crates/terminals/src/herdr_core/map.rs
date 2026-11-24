//! Projects herdr-core wire types into the common model.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

use crate::error::TerminalError;
use crate::events::{BackendEvent, Resync, TerminalChange, TerminalEvent};
use crate::model::{
    AgentKind, AgentState, AgentStatus, BlockedReason, ScreenText, TerminalId, TerminalInfo,
    TerminalLifecycle, TerminalSize,
};
use crate::util::first_non_empty;

use super::wire::{EventParams, FinishedData, Foreground, Status, Terminal};

/// One parsed notification from a core subscription.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreNotice {
    /// A fresh snapshot, first or after missed events.
    Resync(Resync),
    /// One fact change.
    Event(TerminalEvent),
    /// The core is alive.
    Heartbeat {
        /// Its current sequence.
        seq: u64,
    },
}

impl From<CoreNotice> for BackendEvent {
    fn from(notice: CoreNotice) -> Self {
        match notice {
            CoreNotice::Resync(resync) => Self::Resync(resync),
            CoreNotice::Event(event) => Self::Terminal(event),
            CoreNotice::Heartbeat { seq } => Self::Heartbeat { seq },
        }
    }
}

/// Converts a core terminal into the common model.
///
/// `status.msg` and `status.title` are intentionally dropped: they can contain
/// conversation text.
#[must_use]
pub fn terminal_info(terminal: Terminal) -> TerminalInfo {
    let title = first_non_empty([
        terminal.title.as_str(),
        terminal.name.as_deref().unwrap_or_default(),
        terminal.address.as_str(),
    ]);
    let agent_kind = terminal
        .status
        .as_ref()
        .and_then(|status| status.app.as_deref())
        .filter(|app| !app.trim().is_empty())
        .map(AgentKind::normalize);
    let agent_state = terminal.status.as_ref().map(agent_status);
    let finished = terminal.state == "finished";
    TerminalInfo {
        id: TerminalId::new(terminal.address),
        title,
        cwd: (!terminal.cwd.is_empty()).then_some(terminal.cwd),
        labels: terminal.labels,
        agent_kind,
        agent_state,
        lifecycle: if finished {
            TerminalLifecycle::Finished
        } else {
            TerminalLifecycle::Live
        },
        created_ms: terminal.created_ms,
        exit_code: terminal.exit_code,
        finished_reason: if finished { terminal.reason } else { None },
    }
}

/// Converts a core status record into the common model.
#[must_use]
pub fn agent_status(status: &Status) -> AgentStatus {
    let state = AgentState::from_word(&status.state);
    AgentStatus {
        state,
        blocked_reason: if state == AgentState::Blocked {
            status.kind.as_deref().map(BlockedReason::from_word)
        } else {
            None
        },
        progress: status.progress,
    }
}

/// Converts a core `terminal.read` result.
#[must_use]
pub fn screen_text(read: super::wire::ReadResult) -> ScreenText {
    ScreenText {
        text: read.text,
        complete: read.complete,
        origin: crate::model::ReadOrigin::from_core_how(&read.how),
        truncated: false,
    }
}

/// Parses one subscription notification.
///
/// Returns `Ok(None)` for notification methods and event types this crate does
/// not model yet; a malformed payload for a known kind is a protocol error.
///
/// # Errors
///
/// Returns [`TerminalError::Protocol`] when a known event payload does not
/// match its schema.
pub fn parse_notice(method: &str, params: &Value) -> Result<Option<CoreNotice>, TerminalError> {
    match method {
        "heartbeat" => {
            let seq = params.get("seq").and_then(Value::as_u64);
            Ok(seq.map(|seq| CoreNotice::Heartbeat { seq }))
        }
        "missed" => Ok(parse_snapshot(params)?.map(CoreNotice::Resync)),
        "event" => {
            let event: EventParams = serde_json::from_value(params.clone())?;
            if event.kind == "missed" {
                // Some builds send the fresh snapshot as a `missed` event.
                return Ok(parse_snapshot(&event.data)?.map(CoreNotice::Resync));
            }
            map_event(&event).map(|notice| notice.map(CoreNotice::Event))
        }
        _ => Ok(None),
    }
}

fn parse_snapshot(value: &Value) -> Result<Option<Resync>, TerminalError> {
    let Some(terminals) = value.get("terminals") else {
        return Ok(None);
    };
    let seq = value.get("seq").and_then(Value::as_u64).unwrap_or(0);
    let terminals: Vec<Terminal> = serde_json::from_value(terminals.clone())?;
    Ok(Some(Resync {
        seq,
        terminals: terminals.into_iter().map(terminal_info).collect(),
    }))
}

fn map_event(event: &EventParams) -> Result<Option<TerminalEvent>, TerminalError> {
    // `doc` and other global events carry no terminal; they are not terminal
    // state and must not end the stream.
    let Some(terminal) = event.terminal.as_deref() else {
        return Ok(None);
    };
    let terminal = TerminalId::new(terminal);
    let data = &event.data;
    let change = match event.kind.as_str() {
        "created" | "restarted" => {
            let terminal: Terminal = serde_json::from_value(data.clone())?;
            TerminalChange::Upserted {
                terminal: terminal_info(terminal),
            }
        }
        "status" => {
            let status: Option<Status> = data
                .get("status")
                .filter(|value| !value.is_null())
                .cloned()
                .map(serde_json::from_value)
                .transpose()?;
            let Some(status) = status else {
                return Ok(None);
            };
            TerminalChange::Status {
                agent_kind: status
                    .app
                    .as_deref()
                    .filter(|app| !app.trim().is_empty())
                    .map(AgentKind::normalize),
                status: agent_status(&status),
                turn: data.get("turn").and_then(Value::as_u64),
            }
        }
        "turn" => {
            let state = data
                .get("state")
                .and_then(Value::as_str)
                .unwrap_or_default();
            TerminalChange::Turn {
                state: AgentState::from_word(state),
                turn: data.get("turn").and_then(Value::as_u64).unwrap_or(0),
            }
        }
        "size" => {
            let size = TerminalSize::new(u16_field(data, "cols"), u16_field(data, "rows"));
            TerminalChange::Size { size }
        }
        "labels" => {
            #[derive(Deserialize)]
            struct Labels {
                #[serde(default)]
                labels: BTreeMap<String, String>,
            }
            let labels: Labels = serde_json::from_value(data.clone())?;
            TerminalChange::Labels {
                labels: labels.labels,
            }
        }
        "title" => TerminalChange::Title {
            title: data
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        },
        "foreground" => {
            let foreground: Foreground = serde_json::from_value(data.clone())?;
            TerminalChange::Foreground {
                name: foreground.name,
                argv: foreground.argv,
                pid: (foreground.pid != 0).then_some(foreground.pid),
            }
        }
        "finished" | "lost" => {
            let finished: FinishedData = serde_json::from_value(data.clone())?;
            TerminalChange::Finished {
                reason: finished
                    .reason
                    .or_else(|| (event.kind == "lost").then(|| "lost".to_owned())),
                exit_code: finished.exit_code,
                signal: finished.signal,
            }
        }
        "removed" => TerminalChange::Removed,
        _ => return Ok(None),
    };
    Ok(Some(TerminalEvent {
        seq: event.seq,
        terminal,
        change,
    }))
}

fn u16_field(value: &Value, key: &str) -> u16 {
    value
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|number| u16::try_from(number).ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::{CoreNotice, parse_notice, screen_text, terminal_info};
    use crate::events::TerminalChange;
    use crate::jsonrpc::{self, Incoming};
    use crate::model::{AgentState, BlockedReason, ReadOrigin, TerminalLifecycle};
    use crate::test_support::{core_events, core_hello, core_list, core_read, core_subscribe};

    #[test]
    fn a_global_event_without_a_terminal_is_ignored() {
        let notice = parse_notice(
            "event",
            &serde_json::json!({
                "seq": 12,
                "type": "doc",
                "data": { "doc": "herdr-app", "value": {} }
            }),
        )
        .expect("a global event is not an error");

        assert!(notice.is_none());
    }

    #[test]
    fn hello_fixture_reports_the_verified_generation() {
        let hello = core_hello();
        assert_eq!(hello.generation, 1);
        assert_eq!(hello.version, "0.1.19");
        assert!(hello.has_feature("attach"));
        assert!(hello.has_feature("events"));
        assert!(hello.has_feature("status"));
    }

    #[test]
    fn list_fixture_maps_agents_states_and_titles() {
        let terminals: Vec<_> = core_list()
            .terminals
            .into_iter()
            .map(terminal_info)
            .collect();
        assert_eq!(terminals.len(), 3);

        let agent = &terminals[0];
        assert_eq!(agent.id.as_str(), "t11");
        assert_eq!(agent.title, "demo-agent");
        assert_eq!(agent.cwd.as_deref(), Some("/home/dev/project"));
        assert_eq!(
            agent.labels.get("purpose").map(String::as_str),
            Some("demo")
        );
        assert_eq!(
            agent
                .agent_kind
                .as_ref()
                .map(crate::model::AgentKind::as_str),
            Some("claude-code")
        );
        let status = agent.agent_state.as_ref().expect("agent status");
        assert_eq!(status.state, AgentState::Blocked);
        assert_eq!(status.blocked_reason, Some(BlockedReason::Permission));
        assert_eq!(agent.lifecycle, TerminalLifecycle::Live);

        let shell = &terminals[1];
        assert_eq!(shell.title, "t12");
        assert!(shell.agent_kind.is_none());
        assert!(shell.agent_state.is_none());

        let finished = &terminals[2];
        assert_eq!(finished.lifecycle, TerminalLifecycle::Finished);
        assert_eq!(finished.exit_code, Some(0));
        assert_eq!(finished.finished_reason.as_deref(), Some("exited"));
        assert_eq!(
            finished.agent_state.as_ref().map(|s| s.state),
            Some(AgentState::Done)
        );

        let json = serde_json::to_string(&terminals).expect("json");
        assert!(!json.contains("<redacted>"), "sensitive status text leaked");
    }

    #[test]
    fn subscribe_fixture_maps_its_snapshot() {
        let snapshot = core_subscribe();
        assert_eq!(snapshot.seq, 4500);
        let terminal = terminal_info(snapshot.terminals.into_iter().next().expect("terminal"));
        assert_eq!(terminal.id.as_str(), "t11");
        assert_eq!(
            terminal.agent_state.as_ref().map(|state| state.state),
            Some(AgentState::Working)
        );
    }

    #[test]
    fn read_fixture_is_plain_screen_text() {
        let text = screen_text(core_read());
        assert!(text.complete);
        assert_eq!(text.origin, ReadOrigin::Screen);
        assert!(text.text.contains("PROBE_OK"));
        assert!(!text.text.contains('\u{1b}'));
    }

    #[test]
    fn event_fixture_maps_every_documented_change() {
        let notices = core_events();
        assert_eq!(notices.len(), 11, "one notice per fixture line");

        let CoreNotice::Event(created) = &notices[0] else {
            panic!("expected created event");
        };
        assert_eq!(created.terminal.as_str(), "t14");
        assert!(matches!(created.change, TerminalChange::Upserted { .. }));

        let CoreNotice::Event(status) = &notices[3] else {
            panic!("expected status event");
        };
        let TerminalChange::Status {
            agent_kind, status, ..
        } = &status.change
        else {
            panic!("expected status change");
        };
        assert_eq!(
            agent_kind.as_ref().map(crate::model::AgentKind::as_str),
            Some("claude-code")
        );
        assert_eq!(status.state, AgentState::Working);

        let CoreNotice::Event(turn) = &notices[4] else {
            panic!("expected turn event");
        };
        assert_eq!(
            turn.change,
            TerminalChange::Turn {
                state: AgentState::Blocked,
                turn: 1,
            }
        );

        let CoreNotice::Event(size) = &notices[5] else {
            panic!("expected size event");
        };
        assert_eq!(
            size.change,
            TerminalChange::Size {
                size: crate::model::TerminalSize::new(100, 30)
            }
        );

        let CoreNotice::Event(finished) = &notices[7] else {
            panic!("expected finished event");
        };
        assert!(matches!(
            finished.change,
            TerminalChange::Finished {
                exit_code: None,
                signal: Some(1),
                ..
            }
        ));

        assert!(matches!(
            &notices[8],
            CoreNotice::Event(event) if event.change == TerminalChange::Removed
        ));
        assert_eq!(notices[9], CoreNotice::Heartbeat { seq: 4547 });
        let CoreNotice::Resync(resync) = &notices[10] else {
            panic!("expected resync after missed");
        };
        assert_eq!(resync.seq, 4600);
        assert_eq!(resync.terminals[0].title, "after-missed");
    }

    #[test]
    fn status_event_keeps_a_blocked_reason() {
        let params = serde_json::json!({
            "seq": 9,
            "terminal": "t1",
            "type": "status",
            "data": {
                "reported": true,
                "status": {"app": "claude-code", "state": "blocked", "kind": "question"}
            }
        });
        let notice = parse_notice("event", &params)
            .expect("parse")
            .expect("notice");
        let CoreNotice::Event(event) = notice else {
            panic!("expected event");
        };
        let TerminalChange::Status { status, .. } = event.change else {
            panic!("expected status");
        };
        assert_eq!(status.state, AgentState::Blocked);
        assert_eq!(status.blocked_reason, Some(BlockedReason::Question));
    }

    #[test]
    fn missed_event_with_a_snapshot_becomes_a_resync() {
        let params = serde_json::json!({
            "seq": 5001,
            "terminal": "t1",
            "type": "missed",
            "data": {
                "seq": 5000,
                "terminals": [{
                    "address": "t1", "labels": {}, "argv": ["bash"],
                    "cwd": "/tmp", "state": "live", "created_ms": 1,
                    "screen": "exact", "cols": 80, "rows": 24
                }]
            }
        });
        let notice = parse_notice("event", &params)
            .expect("parse")
            .expect("notice");
        assert!(matches!(notice, CoreNotice::Resync(resync) if resync.seq == 5000));
    }

    #[test]
    fn unknown_event_types_are_ignored_not_errors() {
        let params = serde_json::json!({
            "seq": 1, "terminal": "t1", "type": "doc", "data": {}
        });
        assert_eq!(parse_notice("event", &params).expect("parse"), None);
        assert_eq!(parse_notice("mystery", &Value::Null).expect("parse"), None);
    }

    #[test]
    fn notification_lines_round_trip_through_the_jsonrpc_codec() {
        let line = "{\"jsonrpc\":\"2.0\",\"method\":\"heartbeat\",\"params\":{\"seq\":1}}";
        let Incoming::Notification(notification) = jsonrpc::parse_line(line).expect("parse") else {
            panic!("expected notification");
        };
        assert_eq!(
            parse_notice(&notification.method, &notification.params).expect("notice"),
            Some(CoreNotice::Heartbeat { seq: 1 })
        );
    }
}
