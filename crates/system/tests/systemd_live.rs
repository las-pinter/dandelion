//! Live probe for the systemd domain.
//!
//! Every mutating test creates its own transient user unit or its own uniquely
//! named unit file, and removes it afterwards. System units are only read. The
//! root-action probe never touches a real unit: it uses a name that does not
//! exist and expects the missing sudoers policy to stop it first.
//!
//! Run with `cargo test -p dandelion-system --test systemd_live -- --ignored`.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dandelion_system::systemd::{
    ActionMechanism, AllowlistError, JournalFollowRequest, JournalTailRequest, SudoAllowlist,
    SudoError, Systemd, SystemdError, UnitAction, UnitActionRequest, UnitScope, UnitStatus,
};
use tokio::time::{sleep, timeout};

fn unique_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock must be after the epoch")
        .as_nanos();
    format!("{}-{nanos}", std::process::id())
}

async fn connect() -> Systemd {
    Systemd::connect(SudoAllowlist::new())
        .await
        .expect("both systemd managers must be reachable for this live probe")
}

/// Stops the transient probe unit even when an assertion panics.
struct TransientUnitGuard {
    unit: String,
}

impl Drop for TransientUnitGuard {
    fn drop(&mut self) {
        let _ = Command::new("systemctl")
            .args(["--user", "stop", &self.unit])
            .status();
    }
}

fn start_transient_unit(unit_stem: &str) -> String {
    let status = Command::new("systemd-run")
        .args([
            "--user",
            "--unit",
            unit_stem,
            "--collect",
            "/bin/sh",
            "-c",
            "echo dandelion-probe-marker; exec sleep 120",
        ])
        .status()
        .expect("systemd-run must run");
    assert!(status.success(), "creating the probe unit must work");
    format!("{unit_stem}.service")
}

async fn wait_active(systemd: &Systemd, unit: &str) -> UnitStatus {
    for _ in 0..100 {
        if let Ok(status) = systemd.status(UnitScope::User, unit).await
            && status.active_state == "active"
        {
            return status;
        }
        sleep(Duration::from_millis(100)).await;
    }
    panic!("unit {unit} never became active");
}

async fn wait_inactive_or_gone(systemd: &Systemd, unit: &str) {
    for _ in 0..100 {
        match systemd.status(UnitScope::User, unit).await {
            Ok(status) if status.active_state == "inactive" => return,
            Err(_) => return,
            Ok(_) => sleep(Duration::from_millis(100)).await,
        }
    }
    panic!("unit {unit} never stopped");
}

/// Waits until the probe unit's own log line reaches both the tail and a
/// followed stream, then stops the follower.
async fn wait_for_journal_marker(systemd: &Systemd, unit: &str) {
    let mut marker: Option<String> = None;
    for _ in 0..50 {
        let entries = systemd
            .journal_tail(&JournalTailRequest {
                scope: UnitScope::User,
                unit: unit.to_owned(),
                lines: 200,
            })
            .await
            .expect("the journal tail must load");
        if let Some(entry) = entries
            .iter()
            .find(|entry| entry.message.contains("dandelion-probe-marker"))
        {
            marker = Some(entry.message.clone());
            break;
        }
        sleep(Duration::from_millis(100)).await;
    }
    assert!(
        marker.is_some(),
        "the probe unit's journal line must appear"
    );

    let mut stream = systemd
        .journal_follow(&JournalFollowRequest {
            scope: UnitScope::User,
            unit: unit.to_owned(),
            lines: Some(20),
        })
        .expect("following the journal must start");
    let mut followed = None;
    for _ in 0..20 {
        let next = timeout(Duration::from_secs(5), stream.next_entry()).await;
        match next {
            Ok(Some(Ok(entry))) => {
                if entry.message.contains("dandelion-probe-marker") {
                    followed = Some(entry);
                    break;
                }
            }
            Ok(Some(Err(error))) => panic!("journal follow produced an error: {error}"),
            Ok(None) => panic!("journal follow ended before the marker"),
            Err(elapsed) => panic!("journal follow timed out before the marker: {elapsed}"),
        }
    }
    stream.stop().await.expect("stopping the stream must work");
    assert!(
        followed.is_some(),
        "the followed journal must contain the marker"
    );
}

#[tokio::test]
#[ignore = "creates and removes its own transient systemd user unit"]
async fn probe_unit_goes_through_its_full_lifecycle() {
    let systemd = connect().await;
    let unit_stem = format!("dandelion-probe-{}", unique_suffix());
    let _guard = TransientUnitGuard {
        unit: format!("{unit_stem}.service"),
    };

    let unit = start_transient_unit(&unit_stem);
    let status = wait_active(&systemd, &unit).await;
    assert_eq!(status.name, unit);
    assert_eq!(status.scope, UnitScope::User);
    assert_eq!(status.sub_state, "running");
    assert_eq!(
        status.unit_file_state.as_deref(),
        Some("transient"),
        "a systemd-run unit is a transient unit"
    );
    assert!(
        status
            .fragment_path
            .as_deref()
            .is_some_and(|path| path.contains("/transient/")),
        "a transient unit's fragment lives in the transient directory: {:?}",
        status.fragment_path
    );

    let units = systemd
        .list_units(UnitScope::User)
        .await
        .expect("user units must list");
    assert!(
        units.iter().any(|listed| listed.name == unit),
        "the probe unit must be listed"
    );

    wait_for_journal_marker(&systemd, &unit).await;

    let restarted = systemd
        .act(&UnitActionRequest {
            scope: UnitScope::User,
            unit: unit.clone(),
            action: UnitAction::Restart,
        })
        .await
        .expect("restarting our own probe unit must work");
    assert_eq!(restarted.mechanism, ActionMechanism::Dbus);
    wait_active(&systemd, &unit).await;

    let stopped = systemd
        .act(&UnitActionRequest {
            scope: UnitScope::User,
            unit: unit.clone(),
            action: UnitAction::Stop,
        })
        .await
        .expect("stopping our own probe unit must work");
    assert_eq!(stopped.mechanism, ActionMechanism::Dbus);
    wait_inactive_or_gone(&systemd, &unit).await;
}

#[tokio::test]
#[ignore = "reads the real system manager"]
async fn system_units_list_and_status_are_readable() {
    let systemd = connect().await;

    let units = systemd
        .list_units(UnitScope::System)
        .await
        .expect("system units must list");
    assert_ne!(units.len(), 0, "a running host has system units");

    let target = units
        .iter()
        .find(|unit| unit.active_state == "active" && unit.name.ends_with(".service"))
        .expect("some active system service must exist");
    let status = systemd
        .status(UnitScope::System, &target.name)
        .await
        .expect("status of an existing system unit must load");
    assert_eq!(status.name, target.name);
    assert_eq!(status.scope, UnitScope::System);
    assert_eq!(status.active_state, "active");
}

/// Removes the probe unit file and any empty directories the test created.
struct UnitFileGuard {
    unit_name: String,
    unit_path: PathBuf,
    directories: Vec<PathBuf>,
}

impl Drop for UnitFileGuard {
    fn drop(&mut self) {
        let _ = Command::new("systemctl")
            .args(["--user", "disable", &self.unit_name])
            .status();
        let _ = fs::remove_file(&self.unit_path);
        for directory in self.directories.iter().rev() {
            let _ = fs::remove_dir(directory);
        }
    }
}

#[tokio::test]
#[ignore = "writes its own uniquely named unit file under the user config directory"]
async fn probe_unit_file_can_be_enabled_and_disabled() {
    let systemd = connect().await;
    let home = std::env::var("HOME").expect("HOME must be set");
    let unit_stem = format!("dandelion-probe-{}", unique_suffix());
    let unit_name = format!("{unit_stem}.service");

    let base = PathBuf::from(&home).join(".config");
    let unit_directory = base.join("systemd").join("user");
    let mut directories = Vec::new();
    for directory in [
        unit_directory.clone(),
        unit_directory.parent().expect("has a parent").to_path_buf(),
        base.clone(),
    ] {
        if !directory.exists() {
            directories.push(directory);
        }
    }
    fs::create_dir_all(&unit_directory).expect("the user unit directory must be creatable");
    let unit_path = unit_directory.join(&unit_name);
    fs::write(
        &unit_path,
        "[Unit]\nDescription=Dandelion probe\n\n[Service]\nType=oneshot\nExecStart=/bin/true\n\n[Install]\nWantedBy=default.target\n",
    )
    .expect("the probe unit file must be writable");
    let _guard = UnitFileGuard {
        unit_name: unit_name.clone(),
        unit_path,
        directories,
    };

    let enabled = systemd
        .act(&UnitActionRequest {
            scope: UnitScope::User,
            unit: unit_name.clone(),
            action: UnitAction::Enable,
        })
        .await
        .expect("enabling our own probe unit must work");
    assert_eq!(enabled.mechanism, ActionMechanism::Dbus);

    let disabled = systemd
        .act(&UnitActionRequest {
            scope: UnitScope::User,
            unit: unit_name.clone(),
            action: UnitAction::Disable,
        })
        .await
        .expect("disabling our own probe unit must work");
    assert_eq!(disabled.mechanism, ActionMechanism::Dbus);
}

#[tokio::test]
#[ignore = "needs passwordless sudo, which this host does not have"]
async fn system_unit_actions_require_the_sudo_allowlist_and_passwordless_sudo() {
    let probe_unit = "dandelion-probe-sudo-missing.service";
    let mut allowlist = SudoAllowlist::new();
    allowlist
        .allow(probe_unit, vec![UnitAction::Restart])
        .expect("the probe rule must be valid");
    let systemd = Systemd::connect(allowlist)
        .await
        .expect("both systemd managers must be reachable");

    // The probe name does not exist, so even a permitted sudo run cannot touch
    // a real unit; on this host sudo stops at the password prompt first.
    let err = systemd
        .act(&UnitActionRequest {
            scope: UnitScope::System,
            unit: probe_unit.to_owned(),
            action: UnitAction::Restart,
        })
        .await
        .expect_err("passwordless sudo is not configured on this host");
    // Without passwordless sudo the run stops at the password prompt; a host
    // with sudoers configured fails later on the nonexistent unit. Both prove
    // the allowlist gate passed and sudo was reached.
    assert!(
        matches!(
            err,
            SystemdError::Sudo(
                SudoError::Unavailable { .. } | SudoError::Denied { .. } | SudoError::Failed { .. }
            )
        ),
        "unexpected error: {err:?}"
    );

    // An unlisted unit is refused before sudo is even started.
    let err = systemd
        .act(&UnitActionRequest {
            scope: UnitScope::System,
            unit: "nginx.service".to_owned(),
            action: UnitAction::Restart,
        })
        .await
        .expect_err("an unlisted unit must be refused");
    assert!(matches!(
        err,
        SystemdError::Allowlist(AllowlistError::UnitNotListed { .. })
    ));
}
