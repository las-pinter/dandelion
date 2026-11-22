//! Live probe for the process domain.
//!
//! The test starts its own `sleep` child, watches it appear in the process
//! table, reniced it to the lowest priority, stops and continues it, then
//! terminates it. It never signals a process it did not start.
//!
//! Run with `cargo test -p dandelion-system --test process_live -- --ignored`.

use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

use dandelion_system::process::{
    ProcessInfo, ProcessService, ProcessSignal, ProcessState, ReniceRequest, SignalRequest,
};

fn spawn_sleeper(seconds: u32) -> Child {
    Command::new("sleep")
        .arg(seconds.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("sleep must start")
}

fn find(pid: u32) -> Option<ProcessInfo> {
    ProcessService::new()
        .list()
        .expect("the process table must be readable")
        .into_iter()
        .find(|process| process.pid == pid)
}

fn wait_until(timeout: Duration, mut condition: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if condition() {
            return true;
        }
        sleep(Duration::from_millis(50));
    }
    false
}

#[test]
#[ignore = "starts and signals its own real process"]
fn probe_process_goes_through_its_full_lifecycle() {
    let service = ProcessService::new();
    let mut child = spawn_sleeper(60);
    let pid = child.id();

    assert!(
        wait_until(Duration::from_secs(5), || find(pid).is_some()),
        "the spawned sleep must appear in the process list"
    );
    let started = find(pid).expect("process just appeared");
    assert!(
        started.command.contains("sleep"),
        "command: {}",
        started.command
    );
    assert_ne!(started.user, "");
    assert!(started.memory_bytes > 0);
    assert!(started.started_at_unix.is_some_and(|start| start > 0));

    let target = i32::try_from(pid).expect("pid fits i32");
    service
        .renice(&ReniceRequest {
            pid: target,
            priority: 19,
        })
        .expect("lowering our own child's priority must be allowed");
    assert!(
        wait_until(Duration::from_secs(5), || {
            find(pid).is_some_and(|process| process.niceness == 19)
        }),
        "the new nice value must be visible"
    );

    service
        .signal(&SignalRequest {
            pid: target,
            signal: ProcessSignal::Stop,
        })
        .expect("stopping our own child must be allowed");
    assert!(
        wait_until(Duration::from_secs(5), || {
            find(pid).is_some_and(|process| process.state == ProcessState::Stopped)
        }),
        "the process must report the stopped state"
    );

    service
        .signal(&SignalRequest {
            pid: target,
            signal: ProcessSignal::Cont,
        })
        .expect("continuing our own child must be allowed");
    assert!(
        wait_until(Duration::from_secs(5), || {
            find(pid).is_some_and(|process| {
                matches!(
                    process.state,
                    ProcessState::Running | ProcessState::Sleeping
                )
            })
        }),
        "the process must leave the stopped state"
    );

    service
        .signal(&SignalRequest {
            pid: target,
            signal: ProcessSignal::Term,
        })
        .expect("terminating our own child must be allowed");
    let status = child.wait().expect("the child must be reaped");
    assert!(
        !status.success(),
        "SIGTERM must not produce a success status"
    );
    assert!(
        wait_until(Duration::from_secs(5), || find(pid).is_none()),
        "the terminated process must leave the process table"
    );
}
