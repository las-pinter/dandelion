//! Live probes against classic herdr, when its server is running.
//!
//! These are `#[ignore]` like the herdr-core probes. They are read-only and
//! skip themselves when no server answers: classic herdr is not running on the
//! verification machine.

use dandelion_terminals::herdr::ClassicHerdrBackend;
use dandelion_terminals::{TerminalBackend, TerminalFilter};

#[tokio::test]
#[ignore = "live classic herdr, if running"]
async fn detection_reports_the_classic_server() {
    let backend = ClassicHerdrBackend::from_environment();
    let descriptor = backend.detect().await;
    if !descriptor.available {
        eprintln!("classic herdr is not running: {descriptor:?}");
        return;
    }
    assert!(descriptor.version.is_some(), "{descriptor:?}");
    assert!(descriptor.protocol.is_some(), "{descriptor:?}");
}

#[tokio::test]
#[ignore = "live classic herdr, if running"]
async fn listing_reads_the_session_snapshot() {
    let backend = ClassicHerdrBackend::from_environment();
    match backend.list(&TerminalFilter::all()).await {
        Ok(terminals) => {
            for terminal in terminals {
                assert_ne!(terminal.id.as_str(), "");
            }
        }
        Err(error) if matches!(error, dandelion_terminals::TerminalError::Unavailable(_)) => {
            eprintln!("classic herdr is not running: {error}");
        }
        Err(error) => panic!("list failed: {error}"),
    }
}
