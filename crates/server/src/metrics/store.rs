//! SQLite persistence for compact metrics points and minute aggregates.

use dandelion_sysmon::{MetricsPoint, MinuteAggregate};
use serde::de::DeserializeOwned;
use sqlx::SqlitePool;
use thiserror::Error;

/// Metrics storage failures.
#[derive(Debug, Error)]
pub enum MetricsError {
    /// The database rejected an operation.
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    /// A payload could not be encoded or decoded.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Persists metrics history in SQLite.
#[derive(Debug, Clone)]
pub struct MetricsStore {
    pool: SqlitePool,
}

impl MetricsStore {
    /// Wraps an open database pool.
    #[must_use]
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Stores one second-resolution point.
    pub async fn insert_point(&self, point: &MetricsPoint) -> Result<(), MetricsError> {
        sqlx::query("INSERT OR REPLACE INTO metric_seconds (recorded_at, payload) VALUES (?, ?)")
            .bind(to_i64(point.timestamp_ms))
            .bind(serde_json::to_string(point)?)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Stores one completed minute aggregate.
    pub async fn insert_minute(&self, minute: &MinuteAggregate) -> Result<(), MetricsError> {
        sqlx::query("INSERT OR REPLACE INTO metric_minutes (minute_start, payload) VALUES (?, ?)")
            .bind(to_i64(minute.minute_start_ms))
            .bind(serde_json::to_string(minute)?)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Second points whose timestamp is in `[start_ms, end_ms)`.
    pub async fn seconds_between(
        &self,
        start_ms: u64,
        end_ms: u64,
    ) -> Result<Vec<MetricsPoint>, MetricsError> {
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT payload FROM metric_seconds
             WHERE recorded_at >= ? AND recorded_at < ?
             ORDER BY recorded_at",
        )
        .bind(to_i64(start_ms))
        .bind(to_i64(end_ms))
        .fetch_all(&self.pool)
        .await?;
        Ok(decode_all(rows))
    }

    /// Minute aggregates whose start is in `[start_ms, end_ms)`.
    pub async fn minutes_between(
        &self,
        start_ms: u64,
        end_ms: u64,
    ) -> Result<Vec<MinuteAggregate>, MetricsError> {
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT payload FROM metric_minutes
             WHERE minute_start >= ? AND minute_start < ?
             ORDER BY minute_start",
        )
        .bind(to_i64(start_ms))
        .bind(to_i64(end_ms))
        .fetch_all(&self.pool)
        .await?;
        Ok(decode_all(rows))
    }

    /// Drops second and minute rows older than the given timestamps.
    pub async fn prune(
        &self,
        seconds_before: u64,
        minutes_before: u64,
    ) -> Result<(), MetricsError> {
        sqlx::query("DELETE FROM metric_seconds WHERE recorded_at < ?")
            .bind(to_i64(seconds_before))
            .execute(&self.pool)
            .await?;
        sqlx::query("DELETE FROM metric_minutes WHERE minute_start < ?")
            .bind(to_i64(minutes_before))
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

/// Decodes every row it can; an incompatible row is logged and skipped so one
/// bad record cannot break a whole history window.
fn decode_all<T: DeserializeOwned>(rows: Vec<String>) -> Vec<T> {
    let mut values = Vec::with_capacity(rows.len());
    for row in rows {
        match serde_json::from_str(&row) {
            Ok(value) => values.push(value),
            Err(error) => tracing::warn!(%error, "skipping an undecodable metrics row"),
        }
    }
    values
}

fn to_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

#[cfg(test)]
mod tests {
    use dandelion_sysmon::{MetricsPoint, MinuteAggregate};

    use super::MetricsStore;
    use crate::storage;

    async fn store() -> MetricsStore {
        let pool = storage::open_in_memory().await.expect("in-memory database");
        MetricsStore::new(pool)
    }

    fn point(timestamp_ms: u64, cpu_percent: f32) -> MetricsPoint {
        MetricsPoint {
            timestamp_ms,
            cpu_percent,
            ..MetricsPoint::default()
        }
    }

    #[tokio::test]
    async fn points_round_trip_in_time_order() {
        let store = store().await;
        store
            .insert_point(&point(2_000, 20.0))
            .await
            .expect("insert");
        store
            .insert_point(&point(1_000, 10.0))
            .await
            .expect("insert");

        let points = store.seconds_between(0, 3_000).await.expect("read");

        assert_eq!(points.len(), 2);
        assert_eq!(points[0].timestamp_ms, 1_000);
        assert_eq!(points[0].cpu_percent, 10.0);
        assert_eq!(points[1].timestamp_ms, 2_000);
    }

    #[tokio::test]
    async fn ranges_exclude_the_end_and_older_rows() {
        let store = store().await;
        for timestamp in [1_000, 2_000, 3_000] {
            store
                .insert_point(&point(timestamp, 1.0))
                .await
                .expect("insert");
        }

        let points = store.seconds_between(1_000, 3_000).await.expect("read");

        assert_eq!(points.len(), 2);
        assert_eq!(points[0].timestamp_ms, 1_000);
        assert_eq!(points[1].timestamp_ms, 2_000);
    }

    #[tokio::test]
    async fn a_repeated_timestamp_replaces_the_payload() {
        let store = store().await;
        store
            .insert_point(&point(1_000, 10.0))
            .await
            .expect("insert");
        store
            .insert_point(&point(1_000, 90.0))
            .await
            .expect("insert");

        let points = store.seconds_between(0, 2_000).await.expect("read");

        assert_eq!(points.len(), 1);
        assert_eq!(points[0].cpu_percent, 90.0);
    }

    #[tokio::test]
    async fn minute_aggregates_round_trip() {
        let store = store().await;
        let minute = MinuteAggregate {
            minute_start_ms: 60_000,
            sample_count: 60,
            ..MinuteAggregate::default()
        };
        store.insert_minute(&minute).await.expect("insert");

        let minutes = store.minutes_between(0, 120_000).await.expect("read");

        assert_eq!(minutes, vec![minute]);
    }

    #[tokio::test]
    async fn undecodable_rows_are_skipped() {
        let store = store().await;
        store
            .insert_point(&point(1_000, 10.0))
            .await
            .expect("insert");
        sqlx::query("INSERT INTO metric_seconds (recorded_at, payload) VALUES (?, ?)")
            .bind(2_000_i64)
            .bind("not json")
            .execute(&store.pool)
            .await
            .expect("raw insert");

        let points = store.seconds_between(0, 3_000).await.expect("read");

        assert_eq!(points.len(), 1);
        assert_eq!(points[0].timestamp_ms, 1_000);
    }

    #[tokio::test]
    async fn prune_drops_rows_older_than_the_window() {
        let store = store().await;
        store
            .insert_point(&point(1_000, 1.0))
            .await
            .expect("insert");
        store
            .insert_point(&point(10_000, 2.0))
            .await
            .expect("insert");
        store
            .insert_minute(&MinuteAggregate {
                minute_start_ms: 1_000,
                ..MinuteAggregate::default()
            })
            .await
            .expect("insert");
        store
            .insert_minute(&MinuteAggregate {
                minute_start_ms: 10_000,
                ..MinuteAggregate::default()
            })
            .await
            .expect("insert");

        store.prune(5_000, 5_000).await.expect("prune");

        let points = store.seconds_between(0, u64::MAX).await.expect("read");
        let minutes = store.minutes_between(0, u64::MAX).await.expect("read");
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].timestamp_ms, 10_000);
        assert_eq!(minutes.len(), 1);
        assert_eq!(minutes[0].minute_start_ms, 10_000);
    }
}
