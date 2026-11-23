//! Live agent probes against a real terminal backend.
//!
//! Both tests are ignored by default: they need the daemon this machine runs.

use std::time::Duration;

use dandelion_server::agents::{AgentAction, AgentActionRequest, Agents, SpawnAgentRequest};
use dandelion_terminals::{BackendKind, TerminalFilter};

/// Lists the live backend's agents.
#[tokio::test]
#[ignore = "needs a live herdr-core or classic herdr backend"]
async fn the_live_backend_lists_agents() {
    let agents = Agents::connect().await;
    assert!(agents.backend().is_some(), "a backend answers");
    let snapshot = agents.snapshot().await.expect("live snapshot");
    assert!(snapshot.available, "the backend reports itself available");
    let _ = snapshot.agents.len();
}

/// Spawns a probe terminal, reads it, and kills it, through the server layer.
#[tokio::test]
#[ignore = "needs a live herdr-core backend"]
async fn a_live_probe_terminal_is_spawned_read_and_killed() {
    let agents = Agents::connect().await;
    let Some(backend) = agents.backend() else {
        return;
    };
    assert_eq!(
        backend.kind(),
        BackendKind::HerdrCore,
        "this probe needs herdr-core"
    );
    let request = SpawnAgentRequest {
        line: "printf 'dandelion-agent-probe'; sleep 30".to_owned(),
        cwd: None,
        name: Some(format!("dandelion-probe-{}", std::process::id())),
    };

    let info = agents.spawn(&request).await.expect("the probe starts");
    let id = info.id.to_string();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let screen = agents.screen(&id).await.expect("the probe screen reads");
        if screen.text.contains("dandelion-agent-probe") {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the probe printed its marker: {:?}",
            screen.text
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    agents
        .act(&AgentActionRequest {
            terminal: id.clone(),
            action: AgentAction::Kill,
        })
        .await
        .expect("the probe is killed");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let terminals = backend
            .list(&TerminalFilter::all())
            .await
            .expect("the backend lists terminals");
        if !terminals.iter().any(|terminal| terminal.id.as_str() == id) {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "the probe terminal is gone after the kill"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
