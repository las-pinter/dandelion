//! Append-only audit log for control actions.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use sqlx::SqlitePool;
use thiserror::Error;
use utoipa::ToSchema;

/// One control action to record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuditEntry<'a> {
    /// Who performed the action. Authentication replaces `anonymous` later.
    pub actor: &'a str,
    /// Stable action name, such as `process.kill`.
    pub action: &'a str,
    /// What the action was applied to.
    pub target: &'a str,
    /// Optional JSON detail.
    pub detail: Option<&'a str>,
}

/// One recorded action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema, sqlx::FromRow)]
pub struct AuditRecord {
    /// Row id.
    pub id: i64,
    /// Unix epoch milliseconds.
    pub occurred_at_millis: i64,
    /// Who performed the action.
    pub actor: String,
    /// Stable action name.
    pub action: String,
    /// What the action was applied to.
    pub target: String,
    /// Optional JSON detail.
    pub detail: Option<String>,
}

/// Audit storage failures.
#[derive(Debug, Error)]
pub enum AuditError {
    /// The database rejected the write or read.
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
}

/// Append-only audit log.
#[derive(Debug, Clone)]
pub struct AuditLog {
    pool: SqlitePool,
}

impl AuditLog {
    /// Wraps an open database pool.
    #[must_use]
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Opens an in-memory log with migrations applied; used by tests.
    pub async fn in_memory() -> Result<Self, crate::storage::StorageError> {
        Ok(Self::new(crate::storage::open_in_memory().await?))
    }

    /// Records one action and returns its row id.
    pub async fn record(&self, entry: AuditEntry<'_>) -> Result<i64, AuditError> {
        let result = sqlx::query(
            "INSERT INTO audit_log (occurred_at, actor, action, target, detail)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(now_millis())
        .bind(entry.actor)
        .bind(entry.action)
        .bind(entry.target)
        .bind(entry.detail)
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Returns the newest records first.
    pub async fn recent(&self, limit: u32) -> Result<Vec<AuditRecord>, AuditError> {
        let records = sqlx::query_as::<_, AuditRecord>(
            "SELECT id, occurred_at AS occurred_at_millis, actor, action, target, detail
             FROM audit_log ORDER BY id DESC LIMIT ?",
        )
        .bind(i64::from(limit))
        .fetch_all(&self.pool)
        .await?;

        Ok(records)
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use super::{AuditEntry, AuditLog};

    #[tokio::test]
    async fn records_and_reads_one_row() {
        let log = AuditLog::in_memory().await.expect("in-memory audit log");

        let id = log
            .record(AuditEntry {
                actor: "test",
                action: "test.mutate",
                target: "thing",
                detail: Some(r#"{"reason":"test"}"#),
            })
            .await
            .expect("record");
        let records = log.recent(10).await.expect("recent");

        assert!(id > 0);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].actor, "test");
        assert_eq!(records[0].action, "test.mutate");
        assert_eq!(records[0].target, "thing");
        assert_eq!(records[0].detail.as_deref(), Some(r#"{"reason":"test"}"#));
        assert!(records[0].occurred_at_millis > 0);
    }

    #[tokio::test]
    async fn recent_returns_the_newest_first() {
        let log = AuditLog::in_memory().await.expect("in-memory audit log");
        for action in ["first", "second"] {
            log.record(AuditEntry {
                actor: "test",
                action,
                target: "thing",
                detail: None,
            })
            .await
            .expect("record");
        }

        let records = log.recent(10).await.expect("recent");

        assert_eq!(records.len(), 2);
        assert_eq!(records[0].action, "second");
        assert_eq!(records[1].action, "first");
    }
}
