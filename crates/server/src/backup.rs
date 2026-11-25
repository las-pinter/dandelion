//! Consistent database backups.
//!
//! `VACUUM INTO` writes a defragmented copy of a live database without
//! stopping the server, so a backup may run while the service is up. This
//! host has no `sqlite3` binary, and the copy must not miss WAL frames, so the
//! work runs on its own SQLite connection opened without migrations.
//!
//! The privacy rules use Unix file modes; the crate targets Linux, and the
//! non-Unix branches only keep the workspace building elsewhere.

use std::{
    num::NonZeroUsize,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use sqlx::SqlitePool;
use thiserror::Error;

use crate::{config, storage, storage::StorageError};

/// Prefix of every backup file; the rest of the name is a UTC stamp.
const BACKUP_PREFIX: &str = "dandelion-";
/// Suffix of every backup file.
const BACKUP_SUFFIX: &str = ".db";
/// How many same-millisecond names one copy may try before giving up.
const MAX_NAME_ATTEMPTS: u32 = 255;
/// How long a run waits for another backup to finish before failing.
const LOCK_WAIT: std::time::Duration = std::time::Duration::from_secs(60);
/// How many backups the daily timer keeps.
pub const DEFAULT_KEEP: NonZeroUsize = NonZeroUsize::new(7).expect("7 is not zero");

/// Why a backup failed.
#[derive(Debug, Error)]
pub enum BackupError {
    /// The database has never been created.
    #[error("no database at `{0}`; start the server once first")]
    MissingDatabase(PathBuf),
    /// Opening the database failed.
    #[error("open the database")]
    Storage(#[from] StorageError),
    /// The time is before the Unix epoch.
    #[error("the system clock is before 1970, so a backup name cannot be stamped")]
    Clock,
    /// A path has no UTF-8 form, which SQLite's `VACUUM INTO` parameter needs.
    #[error("the path `{}` is not valid UTF-8", .0.display())]
    NonUtf8Path(PathBuf),
    /// A file or directory operation failed.
    #[error("cannot {action} `{path}`: {source}")]
    Io {
        /// What was attempted.
        action: &'static str,
        /// The affected path.
        path: String,
        /// The underlying failure.
        #[source]
        source: std::io::Error,
    },
    /// The copy itself failed.
    #[error("write the backup `{path}`: {source}")]
    Vacuum {
        /// Destination of the copy.
        path: String,
        /// The database failure.
        #[source]
        source: sqlx::Error,
    },
    /// A backup directory chosen by the operator is readable by others.
    #[error(
        "the backup directory `{}` is readable by other users; run `chmod 0700` on it or choose another --out-dir",
        .0.display()
    )]
    ExposedDirectory(PathBuf),
    /// The backup path exists but is not a directory.
    #[error("the backup path `{}` is not a directory", .0.display())]
    NotADirectory(PathBuf),
    /// An empty `--out-dir` would resolve against the process directory.
    #[error("the backup directory path is empty")]
    EmptyOutDir,
}

/// Copies the database of `data_dir` and keeps the newest `keep` copies.
///
/// The destination defaults to `<data-dir>/backups`. The returned path is the
/// copy just written.
pub async fn backup_database(
    data_dir: &Path,
    out_dir: Option<&Path>,
    keep: NonZeroUsize,
) -> Result<PathBuf, BackupError> {
    let database = config::database_path(data_dir);
    let exists = tokio::fs::try_exists(&database)
        .await
        .map_err(|source| io_error("check the database", &database, source))?;
    if !exists {
        return Err(BackupError::MissingDatabase(database));
    }

    let pool = storage::connect_existing(&database).await?;
    if out_dir.is_some_and(|dir| dir.as_os_str().is_empty()) {
        return Err(BackupError::EmptyOutDir);
    }
    let out_dir = out_dir.map_or_else(|| data_dir.join("backups"), Path::to_path_buf);
    ensure_private_dir(&out_dir)?;
    // A manual run and the timer must not prune each other's partial copy.
    // Waiting on another run is a blocking syscall, so it runs off the worker.
    let lock_dir = out_dir.clone();
    let _lock = tokio::task::spawn_blocking(move || lock_backups(&lock_dir))
        .await
        .map_err(|error| {
            io_error(
                "lock the backup directory",
                &out_dir,
                std::io::Error::other(error),
            )
        })??;
    let path = write_copy(&pool, &out_dir).await?;
    prune(&out_dir, keep, &path).await?;
    Ok(path)
}

/// Creates the directory private, or refuses one that is not private already.
///
/// A directory this run creates is `0700` from the moment it exists, so a
/// concurrent run never observes a loose directory to reject. An
/// operator-supplied directory is never chmodded, because it may be shared on
/// purpose; a non-private one is refused instead of silently rewritten. The
/// check runs even after a creation attempt: a recursive create also reports
/// success for a directory that appeared in a race.
fn ensure_private_dir(out_dir: &Path) -> Result<(), BackupError> {
    create_private_dir(out_dir)?;
    let metadata = std::fs::metadata(out_dir)
        .map_err(|source| io_error("inspect the backup directory", out_dir, source))?;
    if !metadata.is_dir() {
        return Err(BackupError::NotADirectory(out_dir.to_path_buf()));
    }
    if is_private(&metadata) {
        Ok(())
    } else {
        Err(BackupError::ExposedDirectory(out_dir.to_path_buf()))
    }
}

/// Creates `dir` and its parents private, if they do not exist yet.
#[cfg(unix)]
fn create_private_dir(dir: &Path) -> Result<(), BackupError> {
    use std::os::unix::fs::DirBuilderExt;

    // A non-recursive create is atomic; the recursive variant is only for
    // missing parents, and its success is not trusted as "private" by the
    // caller, which always re-checks the metadata.
    match std::fs::DirBuilder::new().mode(0o700).create(dir) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .map_err(|source| io_error("create the backup directory", dir, source)),
        Err(source) => Err(io_error("create the backup directory", dir, source)),
    }
}

#[cfg(not(unix))]
fn create_private_dir(dir: &Path) -> Result<(), BackupError> {
    std::fs::create_dir_all(dir)
        .map_err(|source| io_error("create the backup directory", dir, source))
}

#[cfg(unix)]
fn is_private(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;

    // Group and other must hold no permission bit at all.
    metadata.permissions().mode().trailing_zeros() >= 6
}

#[cfg(not(unix))]
fn is_private(_metadata: &std::fs::Metadata) -> bool {
    true
}

/// Holds an exclusive lock on the backup directory for the whole run.
///
/// The lock is waited for at most [`LOCK_WAIT`]; a hung run must not stall the
/// timer forever. The returned file releases the lock when it drops.
fn lock_backups(out_dir: &Path) -> Result<std::fs::File, BackupError> {
    let path = out_dir.join("dandelion-backup.lock");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(|source| io_error("open the backup lock", &path, source))?;
    let deadline = std::time::Instant::now() + LOCK_WAIT;
    loop {
        match rustix::fs::flock(&file, rustix::fs::FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => return Ok(file),
            Err(rustix::io::Errno::WOULDBLOCK) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
            Err(rustix::io::Errno::WOULDBLOCK) => {
                return Err(io_error(
                    "lock the backup directory (another backup is still running)",
                    &path,
                    std::io::Error::from(std::io::ErrorKind::WouldBlock),
                ));
            }
            Err(errno) => {
                return Err(io_error(
                    "lock the backup directory",
                    &path,
                    std::io::Error::from_raw_os_error(errno.raw_os_error()),
                ));
            }
        }
    }
}

async fn write_copy(pool: &SqlitePool, out_dir: &Path) -> Result<PathBuf, BackupError> {
    let stamp = utc_stamp(SystemTime::now())?;
    let path = unique_path(out_dir, &stamp).await?;
    // The copy lands under a temporary name first, so a crash mid-write can
    // never leave a truncated file that retention would treat as a backup.
    let partial = path.with_extension("db.tmp");

    if let Err(error) = copy_into(pool, &partial, &path).await {
        // A handled failure must not accumulate partial copies.
        let _ = tokio::fs::remove_file(&partial).await;
        return Err(error);
    }
    Ok(path)
}

async fn copy_into(pool: &SqlitePool, partial: &Path, path: &Path) -> Result<(), BackupError> {
    let target = partial
        .to_str()
        .ok_or_else(|| BackupError::NonUtf8Path(partial.to_path_buf()))?
        .to_owned();

    sqlx::query("VACUUM INTO ?1")
        .bind(target)
        .execute(pool)
        .await
        .map_err(|source| BackupError::Vacuum {
            path: path.display().to_string(),
            source,
        })?;
    restrict(partial, 0o600).await?;
    // The rename only publishes the directory entry; flush the data first so
    // a crash cannot leave a durable name pointing at unflushed content.
    tokio::fs::File::open(partial)
        .await
        .map_err(|source| io_error("open the backup copy", partial, source))?
        .sync_all()
        .await
        .map_err(|source| io_error("sync the backup copy", partial, source))?;
    tokio::fs::rename(partial, path)
        .await
        .map_err(|source| io_error("finish the backup", path, source))?;
    // Make the directory entry durable. A crash before the next sync could
    // otherwise bring the file back under its `.tmp` name, which prune treats
    // as stale and deletes.
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    tokio::fs::File::open(dir)
        .await
        .map_err(|source| io_error("open the backup directory", dir, source))?
        .sync_all()
        .await
        .map_err(|source| io_error("sync the backup directory", dir, source))?;
    Ok(())
}

/// Picks a name no existing backup or stale partial copy holds.
///
/// Stamps have millisecond precision, so a collision needs two runs in the
/// same millisecond; the counter covers that and keeps the fixed-width stamp
/// as the sort key. A leftover `.tmp` name is skipped too, because
/// `VACUUM INTO` refuses a destination that already exists.
async fn unique_path(out_dir: &Path, stamp: &str) -> Result<PathBuf, BackupError> {
    let mut path = out_dir.join(format!("{BACKUP_PREFIX}{stamp}{BACKUP_SUFFIX}"));
    let mut attempt = 0u32;
    while is_taken(&path).await? {
        if attempt == MAX_NAME_ATTEMPTS {
            return Err(io_error(
                "find a free backup name",
                &path,
                std::io::ErrorKind::AlreadyExists.into(),
            ));
        }
        attempt += 1;
        path = out_dir.join(format!(
            "{BACKUP_PREFIX}{stamp}-{attempt:02}{BACKUP_SUFFIX}"
        ));
    }
    Ok(path)
}

async fn is_taken(path: &Path) -> Result<bool, BackupError> {
    let partial = path.with_extension("db.tmp");
    for candidate in [path.to_path_buf(), partial] {
        if tokio::fs::try_exists(&candidate)
            .await
            .map_err(|source| io_error("check the backup name", &candidate, source))?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// True for a name this module generates: `dandelion-<stamp>[-NN].db`.
///
/// Retention deletes files, so it only ever touches names in that exact form;
/// a foreign `dandelion-notes.db` or the live `dandelion.db` is never a
/// candidate.
fn is_backup_name(name: &str) -> bool {
    let Some(rest) = name.strip_prefix(BACKUP_PREFIX) else {
        return false;
    };
    let Some(rest) = rest.strip_suffix(BACKUP_SUFFIX) else {
        return false;
    };
    let (stamp, counter) = rest
        .split_once('-')
        .map_or((rest, None), |(stamp, counter)| (stamp, Some(counter)));
    is_stamp(stamp) && counter.is_none_or(is_counter)
}

/// True for the temporary name of a copy that was never renamed into place.
fn is_partial_name(name: &str) -> bool {
    name.strip_suffix(".tmp").is_some_and(is_backup_name)
}

fn is_stamp(stamp: &str) -> bool {
    let bytes = stamp.as_bytes();
    bytes.len() == 20
        && bytes[0..8].iter().all(u8::is_ascii_digit)
        && bytes[8] == b'T'
        && bytes[9..15].iter().all(u8::is_ascii_digit)
        && bytes[15] == b'.'
        && bytes[16..19].iter().all(u8::is_ascii_digit)
        && bytes[19] == b'Z'
}

fn is_counter(counter: &str) -> bool {
    !counter.is_empty() && counter.len() <= 3 && counter.bytes().all(|b| b.is_ascii_digit())
}

/// Deletes the oldest copies so that at most `keep` remain.
///
/// Names carry a fixed-width UTC stamp, so sorting the names sorts them in
/// time order across stamps; two copies in the same millisecond differ only
/// by their counter, which is a tie-break, not an age. `newest` is never
/// deleted: a clock set back can stamp a fresh copy as the oldest, and the
/// copy the caller just wrote must survive.
async fn prune(out_dir: &Path, keep: NonZeroUsize, newest: &Path) -> Result<(), BackupError> {
    let protected = newest.file_name().and_then(|name| name.to_str());
    let mut entries = tokio::fs::read_dir(out_dir)
        .await
        .map_err(|source| io_error("read the backup directory", out_dir, source))?;
    let mut names = Vec::new();
    let mut stale = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|source| io_error("read the backup directory", out_dir, source))?
    {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let is_file = entry
            .file_type()
            .await
            .map_err(|source| io_error("inspect a backup", &entry.path(), source))?
            .is_file();
        if !is_file {
            continue;
        }
        if is_partial_name(name) {
            // A killed copy never reaches the rename step; any partial file
            // found here is stale and its space belongs back to the operator.
            stale.push(name.to_owned());
        } else if is_backup_name(name) && Some(name) != protected {
            names.push(name.to_owned());
        }
    }
    for name in stale {
        let path = out_dir.join(&name);
        tokio::fs::remove_file(&path)
            .await
            .map_err(|source| io_error("remove a partial backup", &path, source))?;
    }
    names.sort();
    // The protected copy is not in `names`; count it so `keep` still bounds
    // the directory instead of silently keeping one extra file.
    let excess = names.len().saturating_sub(keep.get().saturating_sub(1));
    for name in names.into_iter().take(excess) {
        let path = out_dir.join(&name);
        tokio::fs::remove_file(&path)
            .await
            .map_err(|source| io_error("remove an old backup", &path, source))?;
    }
    Ok(())
}

#[cfg(unix)]
async fn restrict(path: &Path, mode: u32) -> Result<(), BackupError> {
    use std::os::unix::fs::PermissionsExt;

    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .await
        .map_err(|source| io_error("restrict permissions on", path, source))
}

#[cfg(not(unix))]
#[allow(clippy::unused_async)]
async fn restrict(_path: &Path, _mode: u32) -> Result<(), BackupError> {
    Ok(())
}

fn io_error(action: &'static str, path: &Path, source: std::io::Error) -> BackupError {
    BackupError::Io {
        action,
        path: path.display().to_string(),
        source,
    }
}

/// Formats an instant as a fixed-width UTC stamp such as
/// `20261125T033000.123Z` (20 characters).
///
/// No date library is linked for one format; the names must sort
/// chronologically, so the width is part of the contract.
fn utc_stamp(now: SystemTime) -> Result<String, BackupError> {
    let since_epoch = now
        .duration_since(UNIX_EPOCH)
        .map_err(|_| BackupError::Clock)?;
    let secs = since_epoch.as_secs();
    let (year, month, day) =
        civil_from_days(i64::try_from(secs / 86_400).map_err(|_| BackupError::Clock)?);
    let clock = secs % 86_400;
    Ok(format!(
        "{year:04}{month:02}{day:02}T{:02}{:02}{:02}.{:03}Z",
        clock / 3600,
        (clock % 3600) / 60,
        clock % 60,
        since_epoch.subsec_millis()
    ))
}

/// Converts days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests;
