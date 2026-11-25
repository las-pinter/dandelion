//! Tests for the backup module.

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{
    fs,
    num::NonZeroUsize,
    time::{Duration, UNIX_EPOCH},
};

use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

use super::{backup_database, ensure_private_dir, lock_backups, prune, unique_path, utc_stamp};
use crate::storage;

fn keep(count: usize) -> NonZeroUsize {
    NonZeroUsize::new(count).expect("a positive keep")
}

async fn seeded_database(dir: &std::path::Path) -> (std::path::PathBuf, SqlitePool) {
    let database = dir.join("dandelion.db");
    let pool = storage::open(&database).await.expect("open");
    sqlx::query(
        "INSERT INTO audit_log (occurred_at, actor, action, target) VALUES (1, 'test', 'a', 't')",
    )
    .execute(&pool)
    .await
    .expect("seed");
    (database, pool)
}

#[tokio::test]
async fn a_backup_holds_the_rows_of_the_live_database() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (_database, pool) = seeded_database(dir.path()).await;

    let path = backup_database(dir.path(), None, keep(7))
        .await
        .expect("backup");

    let copy = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::new().filename(&path).read_only(true))
        .await
        .expect("open the copy");
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_log")
        .fetch_one(&copy)
        .await
        .expect("count rows");
    assert_eq!(rows, 1);
    drop(pool);
}

#[cfg(unix)]
#[tokio::test]
async fn a_backup_is_private() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (_database, pool) = seeded_database(dir.path()).await;

    let path = backup_database(dir.path(), None, keep(7))
        .await
        .expect("backup");

    let file = fs::metadata(&path).expect("backup file");
    let out = fs::metadata(dir.path().join("backups")).expect("backup directory");
    assert_eq!(file.permissions().mode() & 0o777, 0o600);
    assert_eq!(out.permissions().mode() & 0o777, 0o700);
    drop(pool);
}

#[tokio::test]
async fn a_missing_database_is_named_in_the_error() {
    let dir = tempfile::tempdir().expect("temp dir");

    let error = backup_database(dir.path(), None, keep(7))
        .await
        .expect_err("no database yet");

    assert!(error.to_string().contains("dandelion.db"));
}

#[cfg(unix)]
#[tokio::test]
async fn an_exposed_backup_directory_is_refused() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (_database, pool) = seeded_database(dir.path()).await;
    let out = dir.path().join("backups");
    fs::create_dir(&out).expect("create a loose directory");
    fs::set_permissions(&out, fs::Permissions::from_mode(0o755)).expect("loosen");

    let error = backup_database(dir.path(), Some(&out), keep(7))
        .await
        .expect_err("a shared directory is refused");

    assert!(error.to_string().contains("chmod 0700"));
    let mode = fs::metadata(&out).expect("directory").permissions().mode() & 0o777;
    assert_eq!(mode, 0o755, "the operator's directory is not rewritten");
    drop(pool);
}

#[cfg(unix)]
#[tokio::test]
async fn a_private_existing_directory_is_used_as_is() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (_database, pool) = seeded_database(dir.path()).await;
    let out = dir.path().join("backups");
    fs::create_dir(&out).expect("create a private directory");
    fs::set_permissions(&out, fs::Permissions::from_mode(0o700)).expect("restrict");

    backup_database(dir.path(), Some(&out), keep(7))
        .await
        .expect("backup");

    assert!(
        fs::read_dir(&out)
            .expect("read")
            .filter_map(Result::ok)
            .any(|entry| entry.file_name().to_string_lossy().ends_with(".db"))
    );
    drop(pool);
}

#[tokio::test]
async fn retention_keeps_the_newest_copies() {
    let dir = tempfile::tempdir().expect("temp dir");
    for stamp in [
        "20260101T000000.000Z",
        "20260102T000000.000Z",
        "20260103T000000.000Z",
    ] {
        fs::write(dir.path().join(format!("dandelion-{stamp}.db")), b"x").expect("write");
    }
    fs::write(dir.path().join("notes.txt"), b"keep me").expect("write");
    let newest = dir.path().join("dandelion-20260103T000000.000Z.db");

    prune(dir.path(), keep(2), &newest).await.expect("prune");

    let mut names: Vec<String> = fs::read_dir(dir.path())
        .expect("read")
        .filter_map(|entry| entry.ok()?.file_name().to_str().map(str::to_owned))
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec![
            "dandelion-20260102T000000.000Z.db".to_owned(),
            "dandelion-20260103T000000.000Z.db".to_owned(),
            "notes.txt".to_owned(),
        ]
    );
}

#[tokio::test]
async fn a_clock_set_back_never_evicts_the_copy_just_written() {
    let dir = tempfile::tempdir().expect("temp dir");
    for stamp in ["20260101T000000.000Z", "20260102T000000.000Z"] {
        fs::write(dir.path().join(format!("dandelion-{stamp}.db")), b"x").expect("write");
    }
    // A clock set back stamps the fresh copy as the oldest name.
    let written = dir.path().join("dandelion-20260101T000000.000Z.db");

    prune(dir.path(), keep(1), &written).await.expect("prune");

    assert!(written.exists());
    assert!(
        !dir.path()
            .join("dandelion-20260102T000000.000Z.db")
            .exists()
    );
}

#[test]
fn a_stamp_is_fixed_width_utc() {
    assert_eq!(
        utc_stamp(UNIX_EPOCH).expect("stamp"),
        "19700101T000000.000Z".to_owned()
    );
    assert_eq!(
        utc_stamp(UNIX_EPOCH + Duration::from_hours(498_768)).expect("stamp"),
        "20261125T000000.000Z".to_owned()
    );
    assert_eq!(
        utc_stamp(UNIX_EPOCH + Duration::from_secs(86_399)).expect("stamp"),
        "19700101T235959.000Z".to_owned()
    );
    assert_eq!(
        utc_stamp(UNIX_EPOCH + Duration::from_millis(1_795_564_800_123)).expect("stamp"),
        "20261125T000000.123Z".to_owned()
    );
    assert_eq!(
        utc_stamp(UNIX_EPOCH + Duration::from_hours(474_768)).expect("stamp"),
        "20240229T000000.000Z".to_owned()
    );
    assert_eq!(
        utc_stamp(UNIX_EPOCH + Duration::from_hours(499_632)).expect("stamp"),
        "20261231T000000.000Z".to_owned()
    );
    assert_eq!(
        utc_stamp(UNIX_EPOCH + Duration::from_hours(499_656)).expect("stamp"),
        "20270101T000000.000Z".to_owned()
    );
}

#[tokio::test]
async fn foreign_names_are_never_pruned() {
    let dir = tempfile::tempdir().expect("temp dir");
    let old = dir.path().join("dandelion-20260101T000000.000Z.db");
    fs::write(&old, b"x").expect("write");
    for foreign in ["dandelion-1.db", "dandelion.db", "dandelion-notes.db"] {
        fs::write(dir.path().join(foreign), b"keep me").expect("write");
    }
    let newest = dir.path().join("dandelion-20260102T000000.000Z.db");
    fs::write(&newest, b"x").expect("write");

    prune(dir.path(), keep(1), &newest).await.expect("prune");

    assert!(!old.exists(), "an old backup still expires");
    for foreign in ["dandelion-1.db", "dandelion.db", "dandelion-notes.db"] {
        assert!(dir.path().join(foreign).exists(), "{foreign} must survive");
    }
}

#[tokio::test]
async fn a_missing_directory_chain_is_created_private() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (_database, pool) = seeded_database(dir.path()).await;
    let out = dir.path().join("a").join("b").join("backups");

    backup_database(dir.path(), Some(&out), keep(7))
        .await
        .expect("the parent chain is created");

    #[cfg(unix)]
    {
        assert_eq!(
            fs::metadata(&out).expect("leaf").permissions().mode() & 0o777,
            0o700
        );
    }
    drop(pool);
}

#[tokio::test]
async fn a_file_in_place_of_the_directory_is_refused() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (_database, pool) = seeded_database(dir.path()).await;
    let out = dir.path().join("backups");
    fs::write(&out, b"not a directory").expect("write a file");

    let error = backup_database(dir.path(), Some(&out), keep(7))
        .await
        .expect_err("a file is not a backup directory");

    assert!(error.to_string().contains("not a directory"));
    drop(pool);
}

#[tokio::test]
async fn a_stale_partial_name_is_skipped() {
    let dir = tempfile::tempdir().expect("temp dir");
    let stamp = "20260101T000000.000Z";
    fs::write(
        dir.path().join(format!("dandelion-{stamp}.db.tmp")),
        b"half a copy",
    )
    .expect("write");

    let path = unique_path(dir.path(), stamp).await.expect("name");

    assert_eq!(
        path.file_name().and_then(|name| name.to_str()),
        Some("dandelion-20260101T000000.000Z-01.db")
    );
}

#[tokio::test]
async fn an_empty_out_dir_is_refused() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (_database, pool) = seeded_database(dir.path()).await;

    let error = backup_database(dir.path(), Some(std::path::Path::new("")), keep(7))
        .await
        .expect_err("an empty path is refused");

    assert!(error.to_string().contains("empty"));
    drop(pool);
}

#[tokio::test]
async fn a_held_lock_makes_the_next_run_wait() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (_database, pool) = seeded_database(dir.path()).await;
    let out = dir.path().join("backups");
    ensure_private_dir(&out).expect("private directory");
    let held = lock_backups(&out).expect("first lock");
    let data_dir = dir.path().to_path_buf();

    let run = tokio::spawn(async move { backup_database(&data_dir, None, keep(7)).await });
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(!run.is_finished(), "the second run waits for the lock");

    drop(held);
    let path = tokio::time::timeout(Duration::from_secs(10), run)
        .await
        .expect("the run finishes once the lock is free")
        .expect("join")
        .expect("backup");
    assert!(path.exists());
    drop(pool);
}

#[tokio::test]
async fn a_stale_partial_copy_is_removed() {
    let dir = tempfile::tempdir().expect("temp dir");
    let stale = dir.path().join("dandelion-20260101T000000.000Z.db.tmp");
    fs::write(&stale, b"half a copy").expect("write");
    let written = dir.path().join("dandelion-20260102T000000.000Z.db");
    fs::write(&written, b"x").expect("write");

    prune(dir.path(), keep(7), &written).await.expect("prune");

    assert!(!stale.exists());
    assert!(written.exists());
}

#[tokio::test]
async fn running_out_of_names_is_an_error() {
    let dir = tempfile::tempdir().expect("temp dir");
    let stamp = "20260101T000000.000Z";
    fs::write(dir.path().join(format!("dandelion-{stamp}.db")), b"x").expect("write");
    for attempt in 1..=super::MAX_NAME_ATTEMPTS {
        fs::write(
            dir.path()
                .join(format!("dandelion-{stamp}-{attempt:02}.db")),
            b"x",
        )
        .expect("write");
    }

    let error = unique_path(dir.path(), stamp)
        .await
        .expect_err("every name is taken");

    assert!(error.to_string().contains("find a free backup name"));
}

#[tokio::test]
async fn a_taken_name_gets_a_counter() {
    let dir = tempfile::tempdir().expect("temp dir");
    let stamp = "20260101T000000.000Z";
    fs::write(dir.path().join(format!("dandelion-{stamp}.db")), b"x").expect("write");

    let path = unique_path(dir.path(), stamp).await.expect("name");

    assert_eq!(
        path.file_name().and_then(|name| name.to_str()),
        Some("dandelion-20260101T000000.000Z-01.db")
    );
}

#[tokio::test]
async fn a_backup_does_not_apply_pending_migrations() {
    let dir = tempfile::tempdir().expect("temp dir");
    let (database, pool) = seeded_database(dir.path()).await;
    let mut before: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&pool)
            .await
            .expect("read versions");
    // Removing the newest applied row makes that migration pending again,
    // the state a backup must leave alone while the server is running.
    let pending = before.pop().expect("at least one migration");
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version = ?1")
        .bind(pending)
        .execute(&pool)
        .await
        .expect("rewind the migration record");
    drop(pool);

    let path = backup_database(dir.path(), None, keep(7))
        .await
        .expect("backup");

    let copy = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(SqliteConnectOptions::new().filename(&path).read_only(true))
        .await
        .expect("open the copy");
    let applied: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&copy)
            .await
            .expect("read versions");
    assert_eq!(applied, before);
    assert!(database.exists());
}
