//! Live system metrics: persistence, the minute buffer, and the collector task.
//!
//! SQLite is the source of truth for history queries. The in-memory
//! [`dandelion_sysmon::History`] only aggregates the open minute, so its ring
//! capacities stay small.

pub mod collector;
pub mod routes;
pub mod store;

use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};

use dandelion_sysmon::{History, MetricsPoint, MinuteAggregate, Snapshot};
use serde::Serialize;
use sqlx::SqlitePool;
use utoipa::ToSchema;

use self::store::MetricsStore;

/// WebSocket topic carrying a full snapshot once per second.
pub const TOPIC: &str = "system.metrics";

/// One live sample: the full observation and its compact history point.
///
/// Publishing both keeps the client's live series on the same derivation as
/// stored history instead of reimplementing it in TypeScript.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct LiveSample {
    /// The full observation.
    pub snapshot: Snapshot,
    /// The compact point derived from it.
    pub point: MetricsPoint,
}

/// One hour of second-resolution rows.
pub const HOUR_MS: u64 = 60 * 60 * 1000;
/// One week of minute aggregates.
pub const WEEK_MS: u64 = 7 * 24 * HOUR_MS;

/// The buffer only needs the open minute and a few neighbours.
const SECOND_CAPACITY: usize = 120;
const MINUTE_CAPACITY: usize = 8;

/// Live metrics shared by the collector and the API.
#[derive(Debug, Clone)]
pub struct Metrics {
    store: MetricsStore,
    inner: Arc<Mutex<Inner>>,
    latest: Arc<RwLock<Option<Snapshot>>>,
}

#[derive(Debug)]
struct Inner {
    history: History,
    persisted_minute: Option<u64>,
}

impl Metrics {
    /// Builds the service over an open database pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            store: MetricsStore::new(pool),
            inner: Arc::new(Mutex::new(Inner {
                history: History::with_capacity(SECOND_CAPACITY, MINUTE_CAPACITY),
                persisted_minute: None,
            })),
            latest: Arc::new(RwLock::new(None)),
        }
    }

    /// The persistence layer.
    #[must_use]
    pub fn store(&self) -> &MetricsStore {
        &self.store
    }

    /// The newest complete snapshot, if the collector has sampled.
    #[must_use]
    pub fn latest(&self) -> Option<Snapshot> {
        read(&self.latest).clone()
    }

    /// Records the newest snapshot for the API.
    pub fn set_latest(&self, snapshot: Snapshot) {
        *write(&self.latest) = Some(snapshot);
    }

    /// Adds a point to the minute buffer and returns a completed minute the
    /// first time that minute appears.
    #[must_use]
    pub fn push(&self, point: MetricsPoint) -> Option<MinuteAggregate> {
        let mut inner = lock(&self.inner);
        inner.history.push(point);
        let minute = inner.history.latest_minute().cloned()?;
        if inner.persisted_minute == Some(minute.minute_start_ms) {
            return None;
        }
        inner.persisted_minute = Some(minute.minute_start_ms);
        Some(minute)
    }
}

fn lock<T>(lock: &Mutex<T>) -> MutexGuard<'_, T> {
    lock.lock().unwrap_or_else(PoisonError::into_inner)
}

fn read<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(PoisonError::into_inner)
}

fn write<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use dandelion_sysmon::MetricsPoint;

    use super::Metrics;
    use crate::storage;

    fn point(timestamp_ms: u64) -> MetricsPoint {
        MetricsPoint {
            timestamp_ms,
            ..MetricsPoint::default()
        }
    }

    #[tokio::test]
    async fn a_completed_minute_is_returned_once() {
        let pool = storage::open_in_memory().await.expect("in-memory database");
        let metrics = Metrics::new(pool);

        assert!(metrics.push(point(0)).is_none());
        assert!(metrics.push(point(30_000)).is_none());
        let minute = metrics.push(point(60_000)).expect("the first minute");
        assert_eq!(minute.minute_start_ms, 0);
        assert_eq!(minute.sample_count, 2);

        assert!(metrics.push(point(61_000)).is_none());
    }

    #[tokio::test]
    async fn the_latest_snapshot_is_kept() {
        let pool = storage::open_in_memory().await.expect("in-memory database");
        let metrics = Metrics::new(pool);
        assert!(metrics.latest().is_none());

        let snapshot = dandelion_sysmon::Snapshot {
            timestamp_ms: 5_000,
            ..dandelion_sysmon::Snapshot::default()
        };
        metrics.set_latest(snapshot);

        assert_eq!(metrics.latest().expect("a snapshot").timestamp_ms, 5_000);
    }
}
