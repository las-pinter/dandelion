//! Discovery tests against a synthetic Claude state directory and `/proc`.

use std::error::Error;
use std::fs;
use std::io;
use std::path::Path;

use dandelion_claude::{ClaudePaths, Session, SessionStatus, discover};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/sessions");

#[test]
fn discovers_live_sessions_and_rejects_stale_records() -> Result<(), Box<dyn Error>> {
    let temporary = tempfile::tempdir()?;
    let claude_dir = temporary.path().join("claude");
    let sessions_dir = claude_dir.join("sessions");
    copy_fixtures(&sessions_dir)?;

    let proc_root = temporary.path().join("proc");
    write_proc(
        &proc_root,
        424_242,
        "7001",
        Some(b"PATH=/usr/bin\0HC_TERMINAL_ID=t42\0"),
    )?;
    write_proc(&proc_root, 424_243, "7002", None)?;
    write_proc(&proc_root, 424_244, "7004", None)?;
    // The recorded start time does not match, so this pid was reused.
    write_proc(&proc_root, 434_343, "9999", None)?;
    write_proc(&proc_root, 444_444, "7004", None)?;
    write_proc(&proc_root, 444_446, "7005", None)?;

    let paths = ClaudePaths::new(&claude_dir).with_proc_root(&proc_root);
    let report = discover(&paths)?;

    let pids: Vec<u32> = report.sessions.iter().map(|session| session.pid).collect();
    assert_eq!(pids, vec![424_242, 424_243, 424_244]);

    let Some(busy) = report.sessions.first() else {
        panic!("expected a busy session");
    };
    assert_eq!(busy.status, Some(SessionStatus::Busy));
    assert_eq!(busy.terminal_id.as_deref(), Some("t42"));
    assert_eq!(busy.name.as_deref(), Some("example-busy"));
    assert_eq!(busy.agent.as_deref(), Some("example-agent"));
    assert_eq!(busy.version.as_deref(), Some("9.9.9"));
    assert_eq!(busy.cwd, "/work/example");

    let Some(waiting) = report.sessions.get(1) else {
        panic!("expected a waiting session");
    };
    assert_eq!(waiting.status, Some(SessionStatus::Waiting));
    assert_eq!(waiting.waiting_for.as_deref(), Some("permission prompt"));
    assert_eq!(waiting.terminal_id, None);

    let Some(unknown) = report.sessions.get(2) else {
        panic!("expected a session with an unknown status");
    };
    assert_eq!(
        unknown.status,
        Some(SessionStatus::Other("hibernating".to_owned()))
    );

    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("434343.json") && warning.contains("reused")),
        "stale pid should be reported: {:?}",
        report.warnings
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("444444.json") && warning.contains("procStart")),
        "missing procStart should be reported: {:?}",
        report.warnings
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("444445.json") && warning.contains("pid")),
        "file name mismatch should be reported: {:?}",
        report.warnings
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("broken.json")),
        "malformed record should be reported: {:?}",
        report.warnings
    );
    assert_eq!(
        report.warnings.len(),
        4,
        "unexpected warnings: {:?}",
        report.warnings
    );
    assert!(
        !report
            .warnings
            .iter()
            .any(|warning| warning.contains(".key")),
        "key files must never be read: {:?}",
        report.warnings
    );
    Ok(())
}

#[test]
fn preserves_unknown_status_in_serialized_output() -> Result<(), Box<dyn Error>> {
    let temporary = tempfile::tempdir()?;
    let claude_dir = temporary.path().join("claude");
    let sessions_dir = claude_dir.join("sessions");
    fs::create_dir_all(&sessions_dir)?;
    fs::copy(
        Path::new(FIXTURES).join("424244.json"),
        sessions_dir.join("424244.json"),
    )?;
    let proc_root = temporary.path().join("proc");
    write_proc(&proc_root, 424_244, "7004", None)?;

    let paths = ClaudePaths::new(&claude_dir).with_proc_root(&proc_root);
    let report = discover(&paths)?;

    let Some(session) = report.sessions.first() else {
        panic!("expected one session");
    };
    let value = serde_json::to_value(session)?;
    assert_eq!(value.get("status"), Some(&serde_json::json!("hibernating")));
    Ok(())
}

#[test]
fn warnings_do_not_quote_record_contents() -> Result<(), Box<dyn Error>> {
    let temporary = tempfile::tempdir()?;
    let claude_dir = temporary.path().join("claude");
    let sessions_dir = claude_dir.join("sessions");
    fs::create_dir_all(&sessions_dir)?;
    fs::write(
        sessions_dir.join("999999.json"),
        r#"{"pid":"SENTINEL","sessionId":"x","cwd":"/x"}"#,
    )?;
    let paths = ClaudePaths::new(&claude_dir);

    let report = discover(&paths)?;

    assert_eq!(report.sessions, Vec::<Session>::new());
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("999999.json")),
        "the file should be reported: {:?}",
        report.warnings
    );
    assert!(
        !report
            .warnings
            .iter()
            .any(|warning| warning.contains("SENTINEL")),
        "a warning quoted the rejected value: {:?}",
        report.warnings
    );
    Ok(())
}

#[test]
fn missing_registry_is_an_empty_report() -> Result<(), Box<dyn Error>> {
    let temporary = tempfile::tempdir()?;
    let paths = ClaudePaths::new(temporary.path().join("absent"));

    let report = discover(&paths)?;

    assert_eq!(report.sessions, Vec::<Session>::new());
    assert_eq!(report.warnings, Vec::<String>::new());
    Ok(())
}

fn copy_fixtures(target: &Path) -> io::Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(FIXTURES)? {
        let entry = entry?;
        let source = entry.path();
        if source.is_file() {
            fs::copy(&source, target.join(entry.file_name()))?;
        }
    }
    Ok(())
}

fn write_proc(
    proc_root: &Path,
    pid: u32,
    start_time: &str,
    environ: Option<&[u8]>,
) -> io::Result<()> {
    let directory = proc_root.join(pid.to_string());
    fs::create_dir_all(&directory)?;
    fs::write(directory.join("stat"), stat_line(pid, start_time))?;
    if let Some(environ) = environ {
        fs::write(directory.join("environ"), environ)?;
    }
    Ok(())
}

fn stat_line(pid: u32, start_time: &str) -> String {
    format!(
        "{pid} (claude) R 1 {pid} {pid} 0 -1 4194304 100 0 0 0 1 2 0 0 20 0 5 0 {start_time} 100000 123"
    )
}
