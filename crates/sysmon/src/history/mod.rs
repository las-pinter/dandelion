//! Second-resolution metrics with per-minute downsampling and ring eviction.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize, Serializer};
use utoipa::ToSchema;

use crate::types::Snapshot;

/// Seconds of second-resolution history kept by default: one hour.
pub const SECONDS_PER_HOUR: usize = 60 * 60;

/// Minute aggregates kept by default: one week.
pub const MINUTES_PER_WEEK: usize = 7 * 24 * 60;

const MILLIS_PER_MINUTE: u64 = 60 * 1000;

mod metrics;

pub use metrics::{MetricsPoint, MinuteAggregate, Stat};

/// A serialisable copy of everything a history holds, oldest sample first.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct HistorySnapshot {
    /// Second-resolution points.
    pub seconds: Vec<MetricsPoint>,
    /// Completed minute aggregates.
    pub minutes: Vec<MinuteAggregate>,
}

/// Bounded in-memory metrics history.
#[derive(Debug, Clone)]
pub struct History {
    second_capacity: usize,
    minute_capacity: usize,
    seconds: VecDeque<MetricsPoint>,
    minutes: VecDeque<MinuteAggregate>,
    open_minute: Option<u64>,
    open_points: Vec<MetricsPoint>,
    latest_timestamp: Option<u64>,
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

impl History {
    /// Creates a history with one hour of seconds and one week of minutes.
    #[must_use]
    pub fn new() -> Self {
        Self::with_capacity(SECONDS_PER_HOUR, MINUTES_PER_WEEK)
    }

    /// Creates a history with custom ring capacities, useful for tests.
    #[must_use]
    pub fn with_capacity(second_capacity: usize, minute_capacity: usize) -> Self {
        Self {
            second_capacity,
            minute_capacity,
            seconds: VecDeque::with_capacity(second_capacity),
            minutes: VecDeque::with_capacity(minute_capacity),
            open_minute: None,
            open_points: Vec::new(),
            latest_timestamp: None,
        }
    }

    /// Records the compact point of a full snapshot.
    pub fn ingest(&mut self, snapshot: &Snapshot) {
        self.push(MetricsPoint::from_snapshot(snapshot));
    }

    /// Records one point and rolls the previous minute over when it ends.
    ///
    /// Points must arrive in timestamp order; a point at or before the latest
    /// timestamp is ignored.
    pub fn push(&mut self, point: MetricsPoint) {
        if self
            .latest_timestamp
            .is_some_and(|latest| point.timestamp_ms <= latest)
        {
            return;
        }

        let minute = point.timestamp_ms - point.timestamp_ms % MILLIS_PER_MINUTE;
        match self.open_minute {
            Some(open) if minute > open => {
                self.minutes
                    .push_back(MinuteAggregate::from_points(open, &self.open_points));
                trim(&mut self.minutes, self.minute_capacity);
                self.open_points.clear();
                self.open_minute = Some(minute);
            }
            None => self.open_minute = Some(minute),
            Some(_) => {}
        }

        self.open_points.push(point);
        self.seconds.push_back(point);
        trim(&mut self.seconds, self.second_capacity);
        self.latest_timestamp = Some(point.timestamp_ms);
    }

    /// The completed minute aggregates that start within `[start_ms, end_ms)`.
    #[must_use]
    pub fn minutes_between(&self, start_ms: u64, end_ms: u64) -> Vec<MinuteAggregate> {
        in_range(
            &self.minutes,
            |minute| minute.minute_start_ms,
            start_ms,
            end_ms,
        )
    }

    /// The second points whose timestamp falls in `[start_ms, end_ms)`.
    #[must_use]
    pub fn seconds_between(&self, start_ms: u64, end_ms: u64) -> Vec<MetricsPoint> {
        in_range(&self.seconds, |point| point.timestamp_ms, start_ms, end_ms)
    }

    /// Number of second-resolution points held.
    #[must_use]
    pub fn second_count(&self) -> usize {
        self.seconds.len()
    }

    /// Number of completed minute aggregates held.
    #[must_use]
    pub fn minute_count(&self) -> usize {
        self.minutes.len()
    }

    /// The newest completed minute aggregate, if one has rolled over.
    #[must_use]
    pub fn latest_minute(&self) -> Option<&MinuteAggregate> {
        self.minutes.back()
    }

    /// Copies the whole history into a serialisable form, oldest first.
    #[must_use]
    pub fn snapshot(&self) -> HistorySnapshot {
        HistorySnapshot {
            seconds: self.seconds.iter().copied().collect(),
            minutes: self.minutes.iter().cloned().collect(),
        }
    }
}

impl Serialize for History {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.snapshot().serialize(serializer)
    }
}

/// A history is published in `OpenAPI` with the shape of its snapshot.
impl utoipa::PartialSchema for History {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        <HistorySnapshot as utoipa::PartialSchema>::schema()
    }
}

impl utoipa::ToSchema for History {
    fn name() -> std::borrow::Cow<'static, str> {
        <HistorySnapshot as utoipa::ToSchema>::name()
    }

    fn schemas(
        schemas: &mut Vec<(
            String,
            utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
        )>,
    ) {
        <HistorySnapshot as utoipa::ToSchema>::schemas(schemas);
    }
}

/// Finds the logical start of a range and walks forward while keys are below
/// the end. The deque is time-ordered, so both steps are cheap.
fn in_range<T: Clone>(
    items: &VecDeque<T>,
    key: impl Fn(&T) -> u64,
    start_ms: u64,
    end_ms: u64,
) -> Vec<T> {
    let first = items.partition_point(|item| key(item) < start_ms);

    items
        .iter()
        .skip(first)
        .take_while(|item| key(item) < end_ms)
        .cloned()
        .collect()
}

fn trim<T>(items: &mut VecDeque<T>, capacity: usize) {
    while items.len() > capacity {
        items.pop_front();
    }
}

#[cfg(test)]
mod tests {
    use super::{History, MetricsPoint, Stat};
    use crate::types::{Cpu, DiskMount, LoadAverage, Memory, NetworkInterface, Snapshot};

    fn point(timestamp_ms: u64, cpu_percent: f32) -> MetricsPoint {
        MetricsPoint {
            timestamp_ms,
            cpu_percent,
            memory_used_bytes: 1_000,
            memory_total_bytes: 2_000,
            load1: 0.5,
            rx_bytes_per_sec: 10,
            tx_bytes_per_sec: 20,
            read_bytes_per_sec: 30,
            write_bytes_per_sec: 40,
        }
    }

    #[test]
    fn minute_rollover_uses_the_minute_start_as_its_key() {
        let mut history = History::with_capacity(10, 10);
        history.push(point(0, 10.0));
        history.push(point(30_000, 30.0));
        history.push(point(60_000, 50.0));

        let minutes = history.minutes_between(0, 120_000);

        assert_eq!(minutes.len(), 1);
        assert_eq!(minutes[0].minute_start_ms, 0);
        assert_eq!(minutes[0].sample_count, 2);
        assert_eq!(
            minutes[0].cpu_percent,
            Stat {
                avg: 20.0,
                max: 30.0
            }
        );
        assert_eq!(history.minute_count(), 1);
    }

    #[test]
    #[allow(clippy::assert_is_empty)]
    fn open_minute_is_not_reported_until_it_completes() {
        let mut history = History::with_capacity(10, 10);
        history.push(point(0, 10.0));
        history.push(point(1_000, 20.0));

        assert!(history.minutes_between(0, u64::MAX).is_empty());
        assert_eq!(history.seconds_between(0, u64::MAX).len(), 2);
    }

    #[test]
    fn latest_minute_appears_only_after_the_minute_closes() {
        let mut history = History::with_capacity(10, 10);
        history.push(point(0, 10.0));
        assert!(history.latest_minute().is_none());

        history.push(point(60_000, 20.0));

        let minute = history.latest_minute().expect("a completed minute");
        assert_eq!(minute.minute_start_ms, 0);
        assert_eq!(minute.sample_count, 1);
    }

    #[test]
    fn second_ring_evicts_the_oldest_samples() {
        let mut history = History::with_capacity(3, 10);
        for second in 0..5 {
            history.push(point(second * 1_000, 10.0));
        }

        let points = history.seconds_between(0, u64::MAX);
        let timestamps: Vec<u64> = points.iter().map(|point| point.timestamp_ms).collect();

        assert_eq!(history.second_count(), 3);
        assert_eq!(timestamps, vec![2_000, 3_000, 4_000]);
    }

    #[test]
    fn minute_ring_evicts_the_oldest_aggregates() {
        let mut history = History::with_capacity(10, 2);
        for minute in 0..5 {
            history.push(point(minute * 60_000, 10.0));
            history.push(point(minute * 60_000 + 30_000, 30.0));
        }

        let minutes = history.minutes_between(0, u64::MAX);
        let starts: Vec<u64> = minutes
            .iter()
            .map(|minute| minute.minute_start_ms)
            .collect();

        assert_eq!(history.minute_count(), 2);
        assert_eq!(starts, vec![120_000, 180_000]);
    }

    #[test]
    fn time_range_is_half_open() {
        let mut history = History::with_capacity(10, 0);
        history.push(point(1_000, 10.0));
        history.push(point(2_000, 20.0));
        history.push(point(3_000, 30.0));

        let points = history.seconds_between(2_000, 3_000);

        assert_eq!(points.len(), 1);
        assert_eq!(points[0].timestamp_ms, 2_000);
    }

    #[test]
    fn out_of_order_points_are_ignored() {
        let mut history = History::with_capacity(10, 10);
        history.push(point(2_000, 10.0));
        history.push(point(1_000, 20.0));
        history.push(point(2_000, 30.0));

        let points = history.seconds_between(0, u64::MAX);

        assert_eq!(points.len(), 1);
        assert_eq!(points[0].cpu_percent, 10.0);
    }

    #[test]
    fn capacity_zero_still_ignores_out_of_order_points() {
        let mut history = History::with_capacity(0, 10);
        history.push(point(2_000, 10.0));
        history.push(point(1_000, 99.0));
        history.push(point(30_000, 20.0));
        history.push(point(60_000, 30.0));

        let minutes = history.minutes_between(0, u64::MAX);

        assert_eq!(history.second_count(), 0);
        assert_eq!(minutes.len(), 1);
        assert_eq!(minutes[0].sample_count, 2);
        assert_eq!(minutes[0].cpu_percent.max, 20.0);
    }

    #[test]
    fn ingest_sums_interfaces_and_counts_each_device_once() {
        let mut snapshot = Snapshot {
            cpu: Cpu {
                usage_percent: 12.0,
                load: LoadAverage {
                    one: 0.75,
                    ..LoadAverage::default()
                },
                ..Cpu::default()
            },
            memory: Memory {
                total_bytes: 2_000,
                used_bytes: 1_000,
                ..Memory::default()
            },
            ..Snapshot::default()
        };
        snapshot.network = vec![
            NetworkInterface {
                name: "eth0".to_owned(),
                rx_bytes_per_sec: 100,
                tx_bytes_per_sec: 200,
            },
            NetworkInterface {
                name: "lo".to_owned(),
                rx_bytes_per_sec: 5,
                tx_bytes_per_sec: 6,
            },
        ];
        let disk = |device: &str, device_name: &str, read: u64, write: u64| DiskMount {
            device: device.to_owned(),
            device_name: device_name.to_owned(),
            read_bytes_per_sec: read,
            write_bytes_per_sec: write,
            ..DiskMount::default()
        };
        snapshot.disks = vec![
            disk("/dev/sda2", "sda2", 100, 200),
            disk("/dev/mapper/root", "sda2", 50, 60),
            disk("/dev/sdb1", "sdb1", 7, 8),
        ];

        let mut history = History::with_capacity(10, 10);
        history.ingest(&snapshot);
        let points = history.seconds_between(0, 1);

        assert_eq!(
            points[0],
            MetricsPoint {
                timestamp_ms: 0,
                cpu_percent: 12.0,
                memory_used_bytes: 1_000,
                memory_total_bytes: 2_000,
                load1: 0.75,
                rx_bytes_per_sec: 105,
                tx_bytes_per_sec: 206,
                read_bytes_per_sec: 107,
                write_bytes_per_sec: 208,
            }
        );
    }

    #[test]
    fn history_serialises_both_resolutions() {
        let mut history = History::with_capacity(10, 10);
        history.push(point(0, 10.0));
        history.push(point(60_000, 20.0));

        let value = serde_json::to_value(&history).expect("serialise history");

        assert_eq!(value["seconds"][0]["timestamp_ms"], 0);
        assert_eq!(value["seconds"][1]["cpu_percent"], 20.0);
        assert_eq!(value["minutes"][0]["minute_start_ms"], 0);
        assert_eq!(value["minutes"][0]["cpu_percent"]["max"], 10.0);
    }
}
