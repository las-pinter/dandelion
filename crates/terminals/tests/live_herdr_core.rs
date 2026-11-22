//! Live probes against the herdr-core on this machine.
//!
//! Every test is `#[ignore]`; run them with
//!
//! ```text
//! env -u HC_TERMINAL_ID cargo nextest run -p dandelion-terminals --run-ignored all
//! ```
//!
//! Reads may touch existing terminals with `lines: 0` only. Every write test
//! creates its own terminal labelled `purpose=dandelion-test`, detached from
//! the caller, and removes it afterwards (including on panic, through `Probe`).

use std::collections::BTreeSet;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dandelion_terminals::herdr_core::HerdrCoreBackend;
use dandelion_terminals::{
    AgentState, AttachEvent, AttachMode, AttachOptions, BackendEvent, InterruptResult, KeyName,
    ReadOptions, SendOptions, SpawnSpec, TerminalBackend, TerminalError, TerminalFilter,
    TerminalId, TerminalLifecycle, TerminalSize,
};
use tokio::time::{sleep, timeout};

fn backend() -> HerdrCoreBackend {
    assert!(
        std::env::var_os("HC_TERMINAL_ID").is_none(),
        "unset HC_TERMINAL_ID before running live probes"
    );
    HerdrCoreBackend::from_environment()
}

fn unique_name(tag: &str) -> String {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let nanos = u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
            & 0xffff_ffff,
    )
    .expect("masked nanoseconds fit");
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    // herdr-core names are at most 32 characters of a-z 0-9 - _ .
    format!("dandelion-{tag}-{unique:x}{:x}", nanos & 0xffff_ffff)
}

/// Kills and removes a probe terminal, even when the test panics.
struct Probe {
    backend: HerdrCoreBackend,
    id: TerminalId,
}

impl Probe {
    async fn spawn(backend: &HerdrCoreBackend, tag: &str) -> Self {
        let spec = SpawnSpec::new()
            .argv(["bash", "--norc", "--noprofile"])
            .named(unique_name(tag))
            .label("purpose", "dandelion-test")
            .sized(TerminalSize::new(80, 24))
            .detached();
        let info = backend.spawn(&spec).await.expect("spawn probe");
        assert_eq!(info.lifecycle, TerminalLifecycle::Live);
        Self {
            backend: backend.clone(),
            id: info.id,
        }
    }
}

impl Drop for Probe {
    fn drop(&mut self) {
        let backend = self.backend.clone();
        let id = self.id.clone();
        let cleanup = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("cleanup runtime");
            runtime.block_on(async {
                let _ = backend.kill(&id).await;
                let _ = backend.remove(&id).await;
            });
        });
        let _ = cleanup.join();
    }
}

async fn read_until_contains(backend: &HerdrCoreBackend, id: &TerminalId, needle: &str) -> String {
    let deadline = Duration::from_secs(10);
    let mut text = String::new();
    let started = std::time::Instant::now();
    while started.elapsed() < deadline {
        text = backend
            .read(id, ReadOptions::default())
            .await
            .expect("read probe")
            .text;
        if text.contains(needle) {
            return text;
        }
        sleep(Duration::from_millis(100)).await;
    }
    panic!("{needle:?} never appeared on screen; last text: {text:?}");
}

#[tokio::test]
#[ignore = "live herdr-core"]
async fn detect_reports_a_known_core() {
    let descriptor = backend().detect().await;

    assert!(descriptor.available, "{descriptor:?}");
    assert_eq!(descriptor.access, dandelion_terminals::Access::ReadWrite);
    assert_eq!(
        descriptor.protocol,
        Some(dandelion_terminals::ProtocolId::HerdrCore { generation: 1 })
    );
    assert!(
        descriptor
            .version
            .as_deref()
            .is_some_and(|version| version.starts_with("0.1.")),
        "{descriptor:?}"
    );
}

#[tokio::test]
#[ignore = "live herdr-core"]
async fn listed_terminals_match_hc_ls() {
    let backend = backend();
    let mut last = None;
    for _ in 0..5 {
        let listed: BTreeSet<String> = backend
            .list(&TerminalFilter::all())
            .await
            .expect("list")
            .into_iter()
            .map(|info| info.id.as_str().to_owned())
            .collect();
        let output = Command::new("hc")
            .args(["ls", "--json"])
            .env_remove("HC_TERMINAL_ID")
            .output()
            .expect("hc ls");
        assert!(output.status.success(), "hc ls failed");
        let value: serde_json::Value = serde_json::from_slice(&output.stdout).expect("hc ls json");
        let expected: BTreeSet<String> = value["terminals"]
            .as_array()
            .expect("terminals")
            .iter()
            .filter_map(|terminal| terminal["address"].as_str())
            .map(str::to_owned)
            .collect();
        if listed == expected {
            return;
        }
        last = Some((listed, expected));
        sleep(Duration::from_secs(1)).await;
    }
    panic!("terminal list never matched hc ls: {last:?}");
}

#[tokio::test]
#[ignore = "live herdr-core"]
async fn existing_terminals_read_without_history_and_without_ansi() {
    let backend = backend();
    let terminals = backend.list(&TerminalFilter::all()).await.expect("list");
    let terminal = terminals.first().expect("at least one live terminal");

    let screen = backend
        .read(&terminal.id, ReadOptions::default())
        .await
        .expect("read");

    assert!(
        !screen.text.contains('\u{1b}'),
        "read returned ANSI escapes"
    );
    assert_eq!(screen.origin, dandelion_terminals::ReadOrigin::Screen);
}

#[tokio::test]
#[ignore = "live herdr-core"]
async fn probe_lifecycle_write_interrupt_kill_remove() {
    let backend = backend();
    let probe = Probe::spawn(&backend, "lifecycle").await;
    let id = probe.id.clone();

    let info = backend.get(&id).await.expect("get");
    assert_eq!(
        info.labels.get("purpose").map(String::as_str),
        Some("dandelion-test")
    );

    backend
        .send_text(&id, "echo DANDELION_PROBE_OK", SendOptions::default())
        .await
        .expect("send text");
    read_until_contains(&backend, &id, "DANDELION_PROBE_OK").await;

    let keys: Vec<KeyName> = [
        "e", "c", "h", "o", "space", "K", "E", "Y", "O", "K", "enter",
    ]
    .iter()
    .map(|key| KeyName::new(*key).expect("key name"))
    .collect();
    backend.send_keys(&id, &keys).await.expect("send keys");
    read_until_contains(&backend, &id, "keyok").await;

    backend
        .send_text(&id, "sleep 30", SendOptions::default())
        .await
        .expect("send sleep");
    sleep(Duration::from_millis(400)).await;
    let interrupt = backend.interrupt(&id).await.expect("interrupt");
    assert!(
        matches!(
            interrupt.result,
            InterruptResult::Stopped | InterruptResult::Exited
        ),
        "{interrupt:?}"
    );

    backend.kill(&id).await.expect("kill");
    let finished = backend.get(&id).await.expect("get finished");
    assert_eq!(finished.lifecycle, TerminalLifecycle::Finished);

    backend.remove(&id).await.expect("remove");
    assert!(matches!(
        backend.get(&id).await,
        Err(TerminalError::NotFound(_))
    ));
}

#[tokio::test]
#[ignore = "live herdr-core"]
async fn subscription_reports_a_probes_lifecycle() {
    let backend = backend();
    let mut stream = backend
        .subscribe(&TerminalFilter::all().with_label("purpose", "dandelion-test"))
        .await
        .expect("subscribe");

    let snapshot = timeout(Duration::from_secs(5), stream.next())
        .await
        .expect("snapshot timeout")
        .expect("stream ended")
        .expect("snapshot error");
    assert!(matches!(snapshot, BackendEvent::Resync(_)), "{snapshot:?}");

    let probe = Probe::spawn(&backend, "events").await;
    let id = probe.id.clone();

    let mut saw_upsert = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline && !saw_upsert {
        let event = timeout(Duration::from_secs(10), stream.next())
            .await
            .expect("event timeout")
            .expect("stream ended")
            .expect("event error");
        if let BackendEvent::Terminal(event) = event
            && event.terminal == id
            && matches!(
                event.change,
                dandelion_terminals::TerminalChange::Upserted { .. }
            )
        {
            saw_upsert = true;
        }
    }
    assert!(saw_upsert, "no created event for the probe");

    backend.kill(&id).await.expect("kill");
    backend.remove(&id).await.expect("remove");

    let mut saw_removed = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline && !saw_removed {
        let event = timeout(Duration::from_secs(10), stream.next())
            .await
            .expect("event timeout")
            .expect("stream ended")
            .expect("event error");
        if let BackendEvent::Terminal(event) = event
            && event.terminal == id
            && event.change == dandelion_terminals::TerminalChange::Removed
        {
            saw_removed = true;
        }
    }
    assert!(saw_removed, "no removed event for the probe");
}

#[tokio::test]
#[ignore = "live herdr-core"]
async fn observe_attach_receives_output_frames() {
    let backend = backend();
    let probe = Probe::spawn(&backend, "attach").await;

    let mut session = backend
        .attach(
            &probe.id,
            AttachOptions {
                mode: AttachMode::Observe,
                size: TerminalSize::new(80, 24),
                history: 0,
            },
        )
        .await
        .expect("attach");

    let mut bytes = 0_usize;
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline && bytes == 0 {
        let event = timeout(Duration::from_secs(10), session.next_event())
            .await
            .expect("attach timeout")
            .expect("attach ended")
            .expect("attach error");
        match event {
            AttachEvent::Data(data) => bytes += data.len(),
            AttachEvent::Resized(size) => assert!(size.cols > 0 && size.rows > 0),
            AttachEvent::Closed { reason } => panic!("attach closed early: {reason:?}"),
        }
    }
    assert!(bytes > 0, "observe attach received no data");
    session.close().await.expect("close");
}

#[tokio::test]
#[ignore = "live herdr-core"]
async fn agent_state_word_is_understood() {
    let backend = backend();
    let terminals = backend.list(&TerminalFilter::all()).await.expect("list");
    for terminal in terminals {
        if let Some(status) = &terminal.agent_state {
            assert_ne!(status.state, AgentState::Unknown, "{terminal:?}");
        }
    }
}
