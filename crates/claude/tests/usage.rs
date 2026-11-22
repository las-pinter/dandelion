//! Token accounting tests against hand-made transcripts.

use std::error::Error;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use dandelion_claude::{ClaudePaths, SessionUsage, TokenUsage, UsageTracker};

const SESSION_ID: &str = "00000000-0000-4000-8000-000000000001";
const CWD: &str = "/work/example";
const SLUG: &str = "-work-example";

const M1: &str = r#"{"type":"assistant","message":{"id":"m1","model":"claude-opus-5-5","usage":{"input_tokens":1000000,"output_tokens":1000000}}}"#;

#[test]
fn deduplicates_by_message_id_and_includes_subagents() -> Result<(), Box<dyn Error>> {
    let (_temporary, paths) = project()?;
    let transcript = transcript_path(&paths);
    let main = [
        r#"{"type":"user","message":{"content":[]}}"#,
        M1,
        M1,
        r#"{"type":"assistant","message":{"id":"m2","model":"claude-opus-5-5","usage":{"input_tokens":500000,"output_tokens":0,"cache_read_input_tokens":1000000,"cache_creation_input_tokens":200000}}}"#,
        r#"{"type":"assistant","message":{"id":"m3","model":"mystery-model","usage":{"input_tokens":1000,"output_tokens":1000}}}"#,
        r#"{"type":"assistant","message":{"id":"m4","model":"claude-opus-5-5"}}"#,
        "not json",
    ];
    write_lines(&transcript, &main)?;
    let Some(project_dir) = transcript.parent() else {
        panic!("transcript has no parent");
    };
    let subagent = project_dir
        .join(SESSION_ID)
        .join("subagents")
        .join("agent-1.jsonl");
    write_lines(
        &subagent,
        &[
            r#"{"type":"assistant","message":{"id":"s1","model":"claude-haiku-5-5","usage":{"input_tokens":1000000,"output_tokens":1000000}}}"#,
        ],
    )?;

    let mut tracker = UsageTracker::new();
    let ingested = tracker.update(&paths, SESSION_ID, CWD)?;
    let usage = tracker.session_usage(SESSION_ID);

    assert_eq!(ingested, 4);
    assert_eq!(usage.messages, 4);
    assert_eq!(
        usage
            .models
            .iter()
            .map(|model| model.model.as_str())
            .collect::<Vec<_>>(),
        vec!["claude-haiku-5-5", "claude-opus-5-5", "mystery-model"]
    );

    let Some(opus) = usage.models.iter().find(|m| m.model == "claude-opus-5-5") else {
        panic!("expected opus usage");
    };
    assert_eq!(
        opus.tokens,
        TokenUsage {
            input_tokens: 1_500_000,
            output_tokens: 1_000_000,
            cache_read_input_tokens: 1_000_000,
            cache_creation_input_tokens: 200_000,
        }
    );
    let Some(opus_cost) = opus.cost_usd else {
        panic!("opus should be priced");
    };
    assert!((opus_cost - 27.2).abs() < 1e-9, "opus cost was {opus_cost}");

    let Some(haiku) = usage.models.iter().find(|m| m.model == "claude-haiku-5-5") else {
        panic!("expected haiku usage");
    };
    let Some(haiku_cost) = haiku.cost_usd else {
        panic!("haiku should be priced");
    };
    assert!(
        (haiku_cost - 0.6).abs() < 1e-9,
        "haiku cost was {haiku_cost}"
    );

    let Some(mystery) = usage.models.iter().find(|m| m.model == "mystery-model") else {
        panic!("expected unpriced usage");
    };
    assert_eq!(mystery.cost_usd, None);
    assert_eq!(usage.unpriced_models, vec!["mystery-model"]);
    assert!((usage.cost_usd - 27.8).abs() < 1e-9);
    assert_eq!(usage.tokens.total(), 5_702_000);
    Ok(())
}

#[test]
fn tails_only_complete_appended_lines() -> Result<(), Box<dyn Error>> {
    let (_temporary, paths) = project()?;
    let transcript = transcript_path(&paths);
    write_lines(&transcript, &[M1])?;

    let mut tracker = UsageTracker::new();
    assert_eq!(tracker.update(&paths, SESSION_ID, CWD)?, 1);
    assert_eq!(tracker.update(&paths, SESSION_ID, CWD)?, 0);

    let mut file = OpenOptions::new().append(true).open(&transcript)?;
    write!(
        file,
        r#"{{"type":"assistant","message":{{"id":"m2","model":"claude-opus-5-5","usage":{{"input_tokens":10,"output_tokens":20}}}}}}"#
    )?;
    file.flush()?;
    assert_eq!(tracker.update(&paths, SESSION_ID, CWD)?, 0);

    writeln!(file)?;
    file.flush()?;
    assert_eq!(tracker.update(&paths, SESSION_ID, CWD)?, 1);

    writeln!(file, "{M1}")?;
    file.flush()?;
    assert_eq!(tracker.update(&paths, SESSION_ID, CWD)?, 0);
    assert_eq!(tracker.session_usage(SESSION_ID).messages, 2);
    Ok(())
}

#[test]
fn refuses_an_unsafe_session_id() -> Result<(), Box<dyn Error>> {
    let (_temporary, paths) = project()?;
    let projects = paths.projects_dir();
    write_lines(&projects.join("escape.jsonl"), &[M1])?;

    let mut tracker = UsageTracker::new();

    assert_eq!(tracker.update(&paths, "../escape", CWD)?, 0);
    assert_eq!(tracker.report(), Vec::<SessionUsage>::new());
    Ok(())
}

#[test]
fn forgetting_a_session_drops_its_state_and_offsets() -> Result<(), Box<dyn Error>> {
    let (_temporary, paths) = project()?;
    write_lines(&transcript_path(&paths), &[M1])?;
    let mut tracker = UsageTracker::new();
    assert_eq!(tracker.update(&paths, SESSION_ID, CWD)?, 1);

    tracker.forget(SESSION_ID);

    assert_eq!(
        tracker.session_usage(SESSION_ID),
        SessionUsage::empty(SESSION_ID)
    );
    assert_eq!(tracker.report(), Vec::<SessionUsage>::new());

    // The offset was dropped too, so the transcript is read again.
    assert_eq!(tracker.update(&paths, SESSION_ID, CWD)?, 1);
    assert_eq!(tracker.session_usage(SESSION_ID).messages, 1);
    Ok(())
}

#[test]
fn missing_transcript_is_zero_usage() -> Result<(), Box<dyn Error>> {
    let (_temporary, paths) = project()?;
    let mut tracker = UsageTracker::new();

    assert_eq!(tracker.update(&paths, SESSION_ID, CWD)?, 0);

    let usage = tracker.session_usage(SESSION_ID);
    assert_eq!(usage.tokens, TokenUsage::default());
    assert_eq!(usage.messages, 0);
    Ok(())
}

fn project() -> io::Result<(tempfile::TempDir, ClaudePaths)> {
    let temporary = tempfile::tempdir()?;
    let claude_dir = temporary.path().join("claude");
    fs::create_dir_all(claude_dir.join("projects").join(SLUG))?;
    Ok((temporary, ClaudePaths::new(claude_dir)))
}

fn transcript_path(paths: &ClaudePaths) -> PathBuf {
    paths
        .projects_dir()
        .join(SLUG)
        .join(format!("{SESSION_ID}.jsonl"))
}

fn write_lines(path: &Path, lines: &[&str]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut content = lines.join("\n");
    content.push('\n');
    fs::write(path, content)
}
