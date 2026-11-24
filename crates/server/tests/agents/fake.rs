//! A fake terminal backend and the attach half its tests drive.
//!
//! It records every call the server makes, so the route tests can assert the
//! mapping and the audit trail without a live daemon. The attach session is a
//! real channel pair: the test sends output and reads the commands the socket
//! produced.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use dandelion_claude::ClaudePaths;
use dandelion_server::claude::ClaudeMonitor;
use dandelion_terminals::{
    Access, AgentKind, AgentState, AgentStatus, AttachCommand, AttachEvent, AttachMode,
    AttachOptions, AttachSession, BackendDescriptor, BackendKind, BlockedReason, EventStream,
    InterruptOutcome, InterruptResult, KeyName, ReadOptions, ScreenText, SendOptions, SendOutcome,
    SpawnSpec, TerminalBackend, TerminalError, TerminalFilter, TerminalId, TerminalInfo,
    TerminalLifecycle, TerminalSize,
};
use tokio::sync::mpsc;

pub const ORIGIN: &str = "https://example.test";
pub const COOKIE: &str = "__Host-dandelion_session";
pub const TERMINAL: &str = "t7";

/// A synthetic Claude state directory with one waiting session in `t7`.
pub struct ClaudeFixture {
    /// Keeps the directory alive for the monitor's lifetime.
    _directory: tempfile::TempDir,
    pub monitor: ClaudeMonitor,
}

pub fn claude_fixture() -> ClaudeFixture {
    use std::fs;

    let temporary = tempfile::tempdir().expect("temp dir");
    let claude_dir = temporary.path().join("claude");
    fs::create_dir_all(claude_dir.join("sessions")).expect("sessions dir");
    fs::write(
        claude_dir.join("sessions").join("424242.json"),
        r#"{"pid":424242,"sessionId":"session-424242","cwd":"/work/example","procStart":"7001","version":"9.9.9","name":"example-session","agent":"example-agent","status":"waiting","waitingFor":"permission prompt","updatedAt":1795372085544}"#,
    )
    .expect("session record");
    // A session outside herdr: no HC_TERMINAL_ID in its environment.
    fs::write(
        claude_dir.join("sessions").join("424243.json"),
        r#"{"pid":424243,"sessionId":"session-424243","cwd":"/work/plain","procStart":"7002","version":"9.9.9","name":"plain-shell-session","status":"idle","updatedAt":1795372085544}"#,
    )
    .expect("unmanaged session record");

    let project = claude_dir.join("projects").join("-work-example");
    fs::create_dir_all(&project).expect("project dir");
    fs::write(
        project.join("session-424242.jsonl"),
        concat!(
            r#"{"type":"assistant","message":{"id":"msg_1","model":"claude-opus-5-5","usage":{"input_tokens":10,"output_tokens":5,"cache_read_input_tokens":100,"cache_creation_input_tokens":2}}}"#,
            "\n"
        ),
    )
    .expect("transcript");

    let proc_root = temporary.path().join("proc");
    let proc_dir = proc_root.join("424242");
    fs::create_dir_all(&proc_dir).expect("proc dir");
    fs::write(
        proc_dir.join("stat"),
        "424242 (claude) R 1 424242 424242 0 -1 4194304 100 0 0 0 1 2 0 0 20 0 5 0 7001 100000 123",
    )
    .expect("proc stat");
    fs::write(
        proc_dir.join("environ"),
        b"PATH=/usr/bin\0HC_TERMINAL_ID=t7\0",
    )
    .expect("environ");

    // A live process with no herdr terminal in its environment.
    let plain_proc_dir = proc_root.join("424243");
    fs::create_dir_all(&plain_proc_dir).expect("plain proc dir");
    fs::write(
        plain_proc_dir.join("stat"),
        "424243 (claude) R 1 424243 424243 0 -1 4194304 100 0 0 0 1 2 0 0 20 0 5 0 7002 100000 123",
    )
    .expect("plain proc stat");

    let paths = ClaudePaths::new(&claude_dir).with_proc_root(&proc_root);
    ClaudeFixture {
        _directory: temporary,
        monitor: ClaudeMonitor::from_paths(paths),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Behaviour {
    Normal,
    Blocked,
    Missing,
}

#[derive(Debug, Default)]
pub struct Recorded {
    pub texts: Vec<(String, String)>,
    pub keys: Vec<(String, Vec<String>)>,
    pub interrupted: Vec<String>,
    pub killed: Vec<String>,
    pub attach: Option<AttachOptions>,
}

/// The test-facing half of an attach session.
#[derive(Debug)]
pub struct FakeAttach {
    events: mpsc::Sender<Result<AttachEvent, TerminalError>>,
    commands: mpsc::Receiver<AttachCommand>,
}

impl FakeAttach {
    /// Returns the next command, or panics when the socket sends none.
    pub async fn next_command(&mut self, wait: Duration) -> AttachCommand {
        tokio::time::timeout(wait, self.commands.recv())
            .await
            .expect("a command within the timeout")
            .expect("the attach session is open")
    }

    /// Sends terminal output to the socket.
    pub async fn send_data(&self, bytes: &[u8]) {
        self.events
            .send(Ok(AttachEvent::Data(bytes.to_vec())))
            .await
            .expect("the attach session is open");
    }
}

#[derive(Debug)]
pub struct FakeBackend {
    pub behaviour: Behaviour,
    pub recorded: Mutex<Recorded>,
    attach_side: tokio::sync::Mutex<Option<FakeAttach>>,
}

impl FakeBackend {
    pub fn new(behaviour: Behaviour) -> Arc<Self> {
        Arc::new(Self {
            behaviour,
            recorded: Mutex::new(Recorded::default()),
            attach_side: tokio::sync::Mutex::new(None),
        })
    }

    pub fn terminal() -> TerminalInfo {
        TerminalInfo {
            id: TerminalId::new(TERMINAL),
            title: "claude in dandelion".to_owned(),
            cwd: Some("/home/dev/dandelion".to_owned()),
            labels: BTreeMap::new(),
            agent_kind: Some(AgentKind::normalize("claude-code")),
            agent_state: Some(AgentStatus {
                state: AgentState::Blocked,
                blocked_reason: Some(BlockedReason::Permission),
                progress: None,
            }),
            lifecycle: TerminalLifecycle::Live,
            created_ms: Some(1_700_000_000_000),
            exit_code: None,
            finished_reason: None,
        }
    }

    /// Waits for the attach session the socket opened.
    pub async fn attach_side(&self, wait: Duration) -> FakeAttach {
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            if let Some(side) = self.attach_side.lock().await.take() {
                return side;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "the socket attached to the fake backend"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

#[async_trait]
impl TerminalBackend for FakeBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::HerdrCore
    }

    async fn detect(&self) -> BackendDescriptor {
        BackendDescriptor {
            kind: BackendKind::HerdrCore,
            available: true,
            version: Some("test".to_owned()),
            protocol: None,
            access: Access::ReadWrite,
            detail: None,
        }
    }

    async fn list(&self, _filter: &TerminalFilter) -> Result<Vec<TerminalInfo>, TerminalError> {
        Ok(vec![Self::terminal()])
    }

    async fn get(&self, id: &TerminalId) -> Result<TerminalInfo, TerminalError> {
        Err(TerminalError::NotFound(id.to_string()))
    }

    async fn subscribe(&self, _filter: &TerminalFilter) -> Result<EventStream, TerminalError> {
        Err(TerminalError::Unsupported("subscribe".to_owned()))
    }

    async fn read(
        &self,
        _id: &TerminalId,
        _options: ReadOptions,
    ) -> Result<ScreenText, TerminalError> {
        Ok(ScreenText {
            text: "Allow this tool?".to_owned(),
            complete: true,
            origin: dandelion_terminals::ReadOrigin::Visible,
            truncated: false,
        })
    }

    async fn send_text(
        &self,
        id: &TerminalId,
        text: &str,
        _options: SendOptions,
    ) -> Result<SendOutcome, TerminalError> {
        match self.behaviour {
            Behaviour::Blocked => return Err(TerminalError::Blocked("permission".to_owned())),
            Behaviour::Missing => return Err(TerminalError::NotFound(id.to_string())),
            Behaviour::Normal => {}
        }
        self.recorded
            .lock()
            .expect("recorded")
            .texts
            .push((id.to_string(), text.to_owned()));
        Ok(SendOutcome {
            started: Some(true),
            status: None,
            turn: 3,
        })
    }

    async fn send_keys(&self, id: &TerminalId, keys: &[KeyName]) -> Result<(), TerminalError> {
        self.recorded.lock().expect("recorded").keys.push((
            id.to_string(),
            keys.iter().map(ToString::to_string).collect(),
        ));
        Ok(())
    }

    async fn interrupt(&self, id: &TerminalId) -> Result<InterruptOutcome, TerminalError> {
        self.recorded
            .lock()
            .expect("recorded")
            .interrupted
            .push(id.to_string());
        Ok(InterruptOutcome {
            key: KeyName::enter(),
            result: InterruptResult::Stopped,
        })
    }

    async fn kill(&self, id: &TerminalId) -> Result<(), TerminalError> {
        self.recorded
            .lock()
            .expect("recorded")
            .killed
            .push(id.to_string());
        Ok(())
    }

    async fn remove(&self, _id: &TerminalId) -> Result<(), TerminalError> {
        Ok(())
    }

    async fn spawn(&self, spec: &SpawnSpec) -> Result<TerminalInfo, TerminalError> {
        assert_eq!(spec.line.as_deref(), Some("claude"));
        assert_eq!(spec.cwd.as_deref(), Some("/home/dev/worktree"));
        Ok(Self::terminal())
    }

    async fn attach(
        &self,
        _id: &TerminalId,
        options: AttachOptions,
    ) -> Result<AttachSession, TerminalError> {
        assert_eq!(
            options.mode,
            AttachMode::Observe,
            "the pane must never type into or resize a real agent"
        );
        assert_eq!(
            options.size,
            TerminalSize {
                cols: 120,
                rows: 40
            }
        );
        self.recorded.lock().expect("recorded").attach = Some(options);

        let (event_tx, event_rx) = mpsc::channel(8);
        let (command_tx, command_rx) = mpsc::channel(8);
        *self.attach_side.lock().await = Some(FakeAttach {
            events: event_tx,
            commands: command_rx,
        });
        Ok(AttachSession::from_channels(event_rx, command_tx))
    }
}
