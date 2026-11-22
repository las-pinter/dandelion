//! Compact history data types and their downsampling maths.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::types::Snapshot;

/// One compact metrics sample at a single point in time.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct MetricsPoint {
    /// Unix time in milliseconds.
    pub timestamp_ms: u64,
    /// Total CPU usage in percent.
    pub cpu_percent: f32,
    /// Physical memory in use, in bytes.
    pub memory_used_bytes: u64,
    /// Total physical memory, in bytes.
    pub memory_total_bytes: u64,
    /// One minute load average.
    pub load1: f32,
    /// Network receive rate summed over all interfaces.
    pub rx_bytes_per_sec: u64,
    /// Network transmit rate summed over all interfaces.
    pub tx_bytes_per_sec: u64,
    /// Disk read rate summed over all mounted devices.
    pub read_bytes_per_sec: u64,
    /// Disk write rate summed over all mounted devices.
    pub write_bytes_per_sec: u64,
}

impl MetricsPoint {
    /// Builds a history point from a full snapshot.
    #[must_use]
    pub fn from_snapshot(snapshot: &Snapshot) -> Self {
        let (mut rx, mut tx) = (0_u64, 0_u64);
        for interface in &snapshot.network {
            rx = rx.saturating_add(interface.rx_bytes_per_sec);
            tx = tx.saturating_add(interface.tx_bytes_per_sec);
        }

        let mut seen_devices = BTreeSet::new();
        let (mut read, mut write) = (0_u64, 0_u64);
        for disk in &snapshot.disks {
            // The collector resolves aliases to one diskstats name; fall back
            // to the mount source for snapshots built by other callers.
            let identity = if disk.device_name.is_empty() {
                disk.device.as_str()
            } else {
                disk.device_name.as_str()
            };
            if seen_devices.insert(identity) {
                read = read.saturating_add(disk.read_bytes_per_sec);
                write = write.saturating_add(disk.write_bytes_per_sec);
            }
        }

        Self {
            timestamp_ms: snapshot.timestamp_ms,
            cpu_percent: snapshot.cpu.usage_percent,
            memory_used_bytes: snapshot.memory.used_bytes,
            memory_total_bytes: snapshot.memory.total_bytes,
            load1: snapshot.cpu.load.one,
            rx_bytes_per_sec: rx,
            tx_bytes_per_sec: tx,
            read_bytes_per_sec: read,
            write_bytes_per_sec: write,
        }
    }
}

/// Average and maximum of one metric within a minute.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct Stat {
    /// Arithmetic mean over the samples in the minute.
    pub avg: f64,
    /// Largest sample in the minute.
    pub max: f64,
}

/// One minute of samples reduced to averages and maxima.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ToSchema)]
pub struct MinuteAggregate {
    /// Unix time of the minute's first millisecond, in milliseconds.
    pub minute_start_ms: u64,
    /// Number of second samples that went into this aggregate.
    pub sample_count: u32,
    /// CPU usage in percent.
    pub cpu_percent: Stat,
    /// Physical memory in use, in bytes.
    pub memory_used_bytes: Stat,
    /// One minute load average.
    pub load1: Stat,
    /// Network receive rate in bytes per second.
    pub rx_bytes_per_sec: Stat,
    /// Network transmit rate in bytes per second.
    pub tx_bytes_per_sec: Stat,
    /// Disk read rate in bytes per second.
    pub read_bytes_per_sec: Stat,
    /// Disk write rate in bytes per second.
    pub write_bytes_per_sec: Stat,
}

impl MinuteAggregate {
    /// Downsamples one minute of points into averages and maxima.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn from_points(minute_start_ms: u64, points: &[MetricsPoint]) -> Self {
        Self {
            minute_start_ms,
            sample_count: u32::try_from(points.len()).unwrap_or(u32::MAX),
            cpu_percent: stat(points.iter().map(|point| f64::from(point.cpu_percent))),
            memory_used_bytes: stat(points.iter().map(|point| point.memory_used_bytes as f64)),
            load1: stat(points.iter().map(|point| f64::from(point.load1))),
            rx_bytes_per_sec: stat(points.iter().map(|point| point.rx_bytes_per_sec as f64)),
            tx_bytes_per_sec: stat(points.iter().map(|point| point.tx_bytes_per_sec as f64)),
            read_bytes_per_sec: stat(points.iter().map(|point| point.read_bytes_per_sec as f64)),
            write_bytes_per_sec: stat(points.iter().map(|point| point.write_bytes_per_sec as f64)),
        }
    }
}

fn stat(values: impl Iterator<Item = f64>) -> Stat {
    let mut count = 0_u64;
    let mut sum = 0.0_f64;
    let mut max = f64::NEG_INFINITY;

    for value in values {
        count += 1;
        sum += value;
        if value > max {
            max = value;
        }
    }

    if count == 0 {
        return Stat::default();
    }

    Stat {
        avg: sum / count_as_f64(count),
        max,
    }
}

#[allow(clippy::cast_precision_loss)]
fn count_as_f64(count: u64) -> f64 {
    count as f64
}

#[cfg(test)]
mod tests {
    use super::{MetricsPoint, MinuteAggregate, Stat};

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
    fn minute_aggregate_averages_and_maximises_each_metric() {
        let points = [point(0, 10.0), point(1_000, 30.0), point(2_000, 20.0)];

        let aggregate = MinuteAggregate::from_points(0, &points);

        assert_eq!(aggregate.sample_count, 3);
        assert_eq!(
            aggregate.cpu_percent,
            Stat {
                avg: 20.0,
                max: 30.0
            }
        );
        assert_eq!(aggregate.memory_used_bytes.max, 1_000.0);
        assert_eq!(aggregate.load1.avg, 0.5);
    }

    #[test]
    fn minute_aggregate_of_no_points_is_zeroed() {
        let aggregate = MinuteAggregate::from_points(60_000, &[]);

        assert_eq!(aggregate.minute_start_ms, 60_000);
        assert_eq!(aggregate.sample_count, 0);
        assert_eq!(aggregate.cpu_percent, Stat::default());
    }
}
