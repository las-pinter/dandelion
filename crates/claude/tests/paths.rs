//! Transcript location tests: the project slug rule and the resumed-session
//! fallback.

use std::error::Error;
use std::fs;
use std::path::Path;

use dandelion_claude::{ClaudePaths, find_transcript};

const SESSION_ID: &str = "00000000-0000-4000-8000-000000000001";

#[test]
fn finds_a_transcript_in_the_slug_for_cwd() -> Result<(), Box<dyn Error>> {
    let temporary = tempfile::tempdir()?;
    let paths = ClaudePaths::new(temporary.path().join("claude"));
    let cwd = "/home/dev/.herdr/worktrees/x";
    let transcript = paths
        .projects_dir()
        .join("-home-dev--herdr-worktrees-x")
        .join(format!("{SESSION_ID}.jsonl"));
    write_empty(&transcript)?;

    assert_eq!(find_transcript(&paths, SESSION_ID, cwd), Some(transcript));
    Ok(())
}

#[test]
fn falls_back_to_another_project_directory_for_a_resumed_session() -> Result<(), Box<dyn Error>> {
    let temporary = tempfile::tempdir()?;
    let paths = ClaudePaths::new(temporary.path().join("claude"));
    let transcript = paths
        .projects_dir()
        .join("-original-project")
        .join(format!("{SESSION_ID}.jsonl"));
    write_empty(&transcript)?;

    assert_eq!(
        find_transcript(&paths, SESSION_ID, "/work/new-location"),
        Some(transcript)
    );
    Ok(())
}

#[test]
fn refuses_an_unsafe_session_id_even_when_a_file_exists() -> Result<(), Box<dyn Error>> {
    let temporary = tempfile::tempdir()?;
    let paths = ClaudePaths::new(temporary.path().join("claude"));
    write_empty(&paths.projects_dir().join("escape.jsonl"))?;

    assert_eq!(find_transcript(&paths, "../escape", "/work/example"), None);
    Ok(())
}

fn write_empty(path: &Path) -> Result<(), Box<dyn Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, "")?;
    Ok(())
}
