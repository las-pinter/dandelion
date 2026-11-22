//! SQLite storage: connection pool and embedded migrations.

use std::{path::Path, time::Duration};

use sqlx::{
    SqlitePool,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
};
use thiserror::Error;

/// Storage failures.
#[derive(Debug, Error)]
pub enum StorageError {
    /// The database rejected an operation.
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    /// An embedded migration failed.
    #[error(transparent)]
    Migrate(#[from] sqlx::migrate::MigrateError),
    /// The data directory could not be created.
    #[error("cannot create the data directory `{path}`: {source}")]
    CreateDir {
        /// Directory that could not be created.
        path: String,
        /// The underlying I/O failure.
        #[source]
        source: std::io::Error,
    },
    /// File permissions could not be restricted.
    #[error("cannot restrict permissions on `{path}`: {source}")]
    Permissions {
        /// File or directory whose mode could not be set.
        path: String,
        /// The underlying I/O failure.
        #[source]
        source: std::io::Error,
    },
}

/// Opens the database, creates its directory, and runs pending migrations.
///
/// A directory that this process creates is private (`0700`), and the database
/// file is made owner-only (`0600`) before migrations write anything. The
/// database will hold sessions and the audit log, so no other local user may
/// read or replace it.
pub async fn open(path: &Path) -> Result<SqlitePool, StorageError> {
    if let Some(parent) = path.parent() {
        let existed =
            tokio::fs::try_exists(parent)
                .await
                .map_err(|source| StorageError::CreateDir {
                    path: parent.display().to_string(),
                    source,
                })?;
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|source| StorageError::CreateDir {
                path: parent.display().to_string(),
                source,
            })?;
        if !existed {
            restrict(parent, 0o700).await?;
        }
    }

    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    // Set this before migrations create the WAL and SHM sidecars.
    restrict(path, 0o600).await?;
    migrate(&pool).await?;
    Ok(pool)
}

#[cfg(unix)]
async fn restrict(path: &Path, mode: u32) -> Result<(), StorageError> {
    use std::os::unix::fs::PermissionsExt;

    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .await
        .map_err(|source| StorageError::Permissions {
            path: path.display().to_string(),
            source,
        })
}

#[cfg(not(unix))]
#[allow(clippy::unused_async)]
async fn restrict(_path: &Path, _mode: u32) -> Result<(), StorageError> {
    Ok(())
}

/// Opens a single-connection in-memory database with migrations applied.
///
/// The tests use this instead of touching the real data directory.
pub async fn open_in_memory() -> Result<SqlitePool, StorageError> {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await?;

    migrate(&pool).await?;
    Ok(pool)
}

/// Runs every embedded migration.
pub async fn migrate(pool: &SqlitePool) -> Result<(), StorageError> {
    sqlx::migrate!("./migrations").run(pool).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{StorageError, open, open_in_memory};

    #[tokio::test]
    async fn migrations_create_the_audit_table() {
        let pool = open_in_memory().await.expect("in-memory database");

        let table: Option<String> = sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'audit_log'",
        )
        .fetch_optional(&pool)
        .await
        .expect("read the schema");

        assert_eq!(table.as_deref(), Some("audit_log"));
    }

    #[tokio::test]
    async fn open_creates_the_data_directory_and_file() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let path = dir.path().join("nested/dandelion.db");

        let pool = open(&path).await.expect("open the database");

        assert!(path.is_file());
        pool.close().await;
    }

    #[tokio::test]
    async fn reopening_an_existing_database_keeps_the_schema() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let path = dir.path().join("dandelion.db");

        let first = open(&path).await.expect("first open");
        first.close().await;
        let second = open(&path).await.expect("second open");

        let table: Option<String> = sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'audit_log'",
        )
        .fetch_optional(&second)
        .await
        .expect("read the schema");

        assert_eq!(table.as_deref(), Some("audit_log"));
        second.close().await;
    }

    #[tokio::test]
    async fn open_reports_a_parent_that_is_a_file() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let blocker = dir.path().join("blocker");
        std::fs::write(&blocker, b"not a directory").expect("write the blocker file");

        let result = open(&blocker.join("dandelion.db")).await;

        assert!(matches!(result, Err(StorageError::CreateDir { .. })));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn created_directory_and_database_are_private() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().expect("temporary directory");
        let parent = dir.path().join("private");
        let path = parent.join("dandelion.db");

        let pool = open(&path).await.expect("open the database");
        let dir_mode = std::fs::metadata(&parent)
            .expect("directory metadata")
            .permissions()
            .mode();
        let file_mode = std::fs::metadata(&path)
            .expect("database metadata")
            .permissions()
            .mode();

        assert_eq!(
            dir_mode & 0o077,
            0,
            "data directory is group/other accessible"
        );
        assert_eq!(file_mode & 0o077, 0, "database is group/other accessible");
        pool.close().await;
    }
}
