//! Live host check. Run with `cargo test -p dandelion-claude -- --ignored`.
//!
//! Read-only: discovery never writes under the Claude state directory.

use std::collections::BTreeSet;
use std::error::Error;
use std::process::Command;
use std::thread;
use std::time::Duration;

use dandelion_claude::{ClaudePaths, discover};

#[test]
#[ignore = "reads this host's live Claude Code sessions"]
fn discovered_pids_match_ps() -> Result<(), Box<dyn Error>> {
    let paths = ClaudePaths::from_home()?;
    let mut discovered = BTreeSet::new();
    let mut running = BTreeSet::new();

    // A session may be starting or exiting between the two snapshots.
    for _ in 0..3 {
        let report = discover(&paths)?;
        discovered = report.sessions.iter().map(|session| session.pid).collect();
        running = claude_process_ids()?;
        if discovered == running {
            eprintln!("session pids match ps: {discovered:?}");
            return Ok(());
        }
        thread::sleep(Duration::from_millis(500));
    }

    panic!("discovered session pids {discovered:?} did not match ps claude pids {running:?}");
}

fn claude_process_ids() -> Result<BTreeSet<u32>, Box<dyn Error>> {
    let output = Command::new("ps")
        .args(["-eo", "uid=,pid=,args="])
        .output()?;
    let text = String::from_utf8(output.stdout)?;
    let current_uid = current_uid();
    let mut pids = BTreeSet::new();

    for line in text.lines() {
        let Some((uid, rest)) = line.trim_start().split_once(char::is_whitespace) else {
            continue;
        };
        if current_uid.as_deref().is_some_and(|current| uid != current) {
            continue;
        }
        let rest = rest.trim_start();
        let Some((pid, args)) = rest.split_once(char::is_whitespace) else {
            continue;
        };
        let Ok(pid) = pid.parse::<u32>() else {
            continue;
        };
        let program = args.split_whitespace().next().unwrap_or_default();
        if program.rsplit('/').next().unwrap_or(program) == "claude" {
            pids.insert(pid);
        }
    }
    Ok(pids)
}

fn current_uid() -> Option<String> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|line| line.starts_with("Uid:"))?;
    line.split_whitespace().nth(1).map(str::to_owned)
}
