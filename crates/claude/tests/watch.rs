//! The registry watcher reports changes without modifying the directory.

use std::error::Error;
use std::fs;
use std::time::Duration;

use dandelion_claude::SessionWatcher;

#[test]
fn reports_a_change_in_the_watched_directory() -> Result<(), Box<dyn Error>> {
    let temporary = tempfile::tempdir()?;
    let watcher = SessionWatcher::new(temporary.path())?;

    fs::write(temporary.path().join("424242.json"), "{}")?;

    assert!(watcher.wait_for_change(Duration::from_secs(5))?);
    Ok(())
}

#[test]
fn rejects_a_missing_directory() -> Result<(), Box<dyn Error>> {
    let temporary = tempfile::tempdir()?;
    let missing = temporary.path().join("sessions");

    let result = SessionWatcher::new(&missing);

    assert!(result.is_err());
    Ok(())
}
