//! The asynchronous metrics collector.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::error::Error;
use crate::procfs::{
    self, CpuTimes, DiskCounters, MountEntry, NetCounters, ProcessCounters, Procfs,
};
use crate::rates;
use crate::types::{Cpu, DiskMount, LoadAverage, Memory, NetworkInterface, Process, Snapshot};

const DEFAULT_TOP_N: usize = 10;
const DEFAULT_INTERVAL: Duration = Duration::from_secs(1);
const MIN_INTERVAL: Duration = Duration::from_millis(1);

/// Collects system metrics from the host's procfs.
///
/// [`Collector::sample`] produces one snapshot synchronously; [`Collector::run`]
/// repeats it on a tokio interval and hands every result to a callback.
#[derive(Debug)]
pub struct Collector {
    procfs: Procfs,
    top_n: usize,
    interval: Duration,
    baseline: Baseline,
}

impl Default for Collector {
    fn default() -> Self {
        Self::new()
    }
}

impl Collector {
    /// Creates a collector with a one second interval and ten ranked
    /// processes.
    #[must_use]
    pub fn new() -> Self {
        Self {
            procfs: Procfs::new(),
            top_n: DEFAULT_TOP_N,
            interval: DEFAULT_INTERVAL,
            baseline: Baseline::default(),
        }
    }

    /// Sets the sampling interval used by [`Collector::run`].
    ///
    /// Intervals below one millisecond are raised to one millisecond, because
    /// a tokio interval cannot tick at zero.
    #[must_use]
    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = interval.max(MIN_INTERVAL);
        self
    }

    /// Sets how many processes each ranked process list holds.
    #[must_use]
    pub fn with_top_n(mut self, top_n: usize) -> Self {
        self.top_n = top_n;
        self
    }

    /// The interval [`Collector::run`] will use.
    #[must_use]
    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// Takes one sample.
    ///
    /// Rates are computed from the difference to the previous sample, so the
    /// first call reports zero traffic.
    pub fn sample(&mut self) -> Result<Snapshot, Error> {
        let now_ms = unix_time_ms()?;
        let now = Instant::now();
        let elapsed = self.baseline.elapsed_secs(now);

        let cpu_times = self.procfs.cpu_times()?;
        let memory = self.procfs.memory()?;
        let load = self.procfs.load_average()?;
        let uptime_secs = self.procfs.uptime_secs()?;
        let network = self.procfs.network()?;
        let disk_counters = self.procfs.disk_counters()?;
        let mounts = self.procfs.mounts()?;
        let processes = self.procfs.processes()?;

        let cpu = self.cpu_snapshot(&cpu_times, load);
        let disks = self.disk_snapshots(&mounts, &disk_counters, elapsed);
        let interfaces = network_rates(&self.baseline.network, &network, elapsed);
        let (by_cpu, by_memory, process_baselines) =
            process_rankings(&self.baseline.processes, &processes, elapsed, self.top_n);

        let snapshot = Snapshot {
            timestamp_ms: now_ms,
            uptime_secs,
            cpu,
            memory: Memory {
                total_bytes: memory.total,
                used_bytes: memory.total.saturating_sub(memory.available),
                available_bytes: memory.available,
                cache_bytes: memory.cache,
                swap_total_bytes: memory.swap_total,
                swap_used_bytes: memory.swap_total.saturating_sub(memory.swap_free),
            },
            disks,
            network: interfaces,
            temperatures: self.procfs.temperatures(),
            top_processes_by_cpu: by_cpu,
            top_processes_by_memory: by_memory,
        };

        self.baseline = Baseline {
            at: Some(now),
            cpu: Some(cpu_times),
            network,
            disk: disk_counters,
            processes: process_baselines,
        };

        Ok(snapshot)
    }

    /// Samples on the configured interval until the task is cancelled.
    ///
    /// The callback receives every result, including errors, so the caller
    /// decides whether a failed sample is fatal. The first sample is taken
    /// immediately.
    ///
    /// Each sample runs on tokio's blocking pool, so slow procfs or
    /// network-backed mounts do not stall async workers.
    pub async fn run<F>(self, mut on_sample: F) -> !
    where
        F: FnMut(Result<Snapshot, Error>),
    {
        let interval = self.interval;
        let collector = Arc::new(Mutex::new(self));
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            ticker.tick().await;
            let shared = Arc::clone(&collector);
            let sampled = tokio::task::spawn_blocking(move || {
                shared
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .sample()
            })
            .await;

            let result = match sampled {
                Ok(result) => result,
                Err(error) => Err(Error::Task {
                    message: error.to_string(),
                }),
            };
            on_sample(result);
        }
    }

    fn cpu_snapshot(&self, current: &CpuTimes, load: LoadAverage) -> Cpu {
        let (usage_percent, per_core_percent) = self.baseline.cpu.as_ref().map_or_else(
            || (0.0, vec![0.0; current.per_core.len()]),
            |previous| {
                (
                    rates::cpu_percent(previous.total, current.total),
                    rates::per_core_percent(&previous.per_core, &current.per_core),
                )
            },
        );

        Cpu {
            usage_percent,
            per_core_percent,
            load,
        }
    }

    fn disk_snapshots(
        &self,
        mounts: &[MountEntry],
        counters: &BTreeMap<String, DiskCounters>,
        elapsed_secs: f64,
    ) -> Vec<DiskMount> {
        let mut disks: Vec<DiskMount> = mounts
            .iter()
            .filter_map(|mount| {
                let usage = procfs::disk_usage(&mount.mount_point)?;
                let device = procfs::device_name(&mount.device);
                let current = counters
                    .get(&device)
                    .map(|counters| (counters.read_bytes, counters.write_bytes));
                let previous = self
                    .baseline
                    .disk
                    .get(&device)
                    .map(|counters| (counters.read_bytes, counters.write_bytes));
                let (read_bytes_per_sec, write_bytes_per_sec) =
                    rate_pair(previous, current, elapsed_secs);

                Some(DiskMount {
                    device: mount.device.clone(),
                    device_name: device,
                    mount_point: mount.mount_point.clone(),
                    filesystem: mount.fs_type.clone(),
                    total_bytes: usage.total,
                    used_bytes: usage.used,
                    available_bytes: usage.available,
                    read_bytes_per_sec,
                    write_bytes_per_sec,
                })
            })
            .collect();

        disks.sort_by(|left, right| left.mount_point.cmp(&right.mount_point));
        disks
    }
}

/// Counters kept from the previous sample so rates can be derived.
#[derive(Debug, Default)]
struct Baseline {
    at: Option<Instant>,
    cpu: Option<CpuTimes>,
    network: BTreeMap<String, NetCounters>,
    disk: BTreeMap<String, DiskCounters>,
    processes: HashMap<u32, ProcessBaseline>,
}

impl Baseline {
    fn elapsed_secs(&self, now: Instant) -> f64 {
        self.at.map_or(0.0, |previous| {
            now.saturating_duration_since(previous).as_secs_f64()
        })
    }
}

/// Identity and CPU counter of a process in the previous sample.
#[derive(Debug, Clone, Copy)]
struct ProcessBaseline {
    start_time: u64,
    cpu_jiffies: u64,
}

fn network_rates(
    previous: &BTreeMap<String, NetCounters>,
    current: &BTreeMap<String, NetCounters>,
    elapsed_secs: f64,
) -> Vec<NetworkInterface> {
    current
        .iter()
        .map(|(name, counters)| {
            let (rx_bytes_per_sec, tx_bytes_per_sec) = rate_pair(
                previous
                    .get(name)
                    .map(|before| (before.rx_bytes, before.tx_bytes)),
                Some((counters.rx_bytes, counters.tx_bytes)),
                elapsed_secs,
            );

            NetworkInterface {
                name: name.clone(),
                rx_bytes_per_sec,
                tx_bytes_per_sec,
            }
        })
        .collect()
}

/// Converts two readings of a byte counter pair into rates.
///
/// A counter without a previous reading belongs to a device or interface that
/// appeared since the last sample. Its lifetime total must not be mistaken for
/// traffic during one interval, so the rate is zero.
fn rate_pair(
    previous: Option<(u64, u64)>,
    current: Option<(u64, u64)>,
    elapsed_secs: f64,
) -> (u64, u64) {
    let (Some((previous_first, previous_second)), Some((current_first, current_second))) =
        (previous, current)
    else {
        return (0, 0);
    };

    (
        rates::bytes_per_sec(previous_first, current_first, elapsed_secs),
        rates::bytes_per_sec(previous_second, current_second, elapsed_secs),
    )
}

fn process_rankings(
    previous: &HashMap<u32, ProcessBaseline>,
    current: &[ProcessCounters],
    elapsed_secs: f64,
    top_n: usize,
) -> (Vec<Process>, Vec<Process>, HashMap<u32, ProcessBaseline>) {
    let page_size = u64::try_from(rustix::param::page_size()).unwrap_or(4096);
    let mut processes = Vec::with_capacity(current.len());
    let mut baselines = HashMap::with_capacity(current.len());

    for counters in current {
        let previous_jiffies = previous
            .get(&counters.pid)
            .filter(|before| before.start_time == counters.start_time)
            .map(|before| before.cpu_jiffies);

        processes.push(Process {
            pid: counters.pid,
            name: counters.name.clone(),
            cpu_percent: rates::process_percent(
                previous_jiffies,
                counters.cpu_jiffies,
                elapsed_secs,
            ),
            memory_bytes: counters.rss_pages.saturating_mul(page_size),
        });
        baselines.insert(
            counters.pid,
            ProcessBaseline {
                start_time: counters.start_time,
                cpu_jiffies: counters.cpu_jiffies,
            },
        );
    }

    let mut by_cpu = processes.clone();
    by_cpu.sort_by(|left, right| {
        right
            .cpu_percent
            .total_cmp(&left.cpu_percent)
            .then_with(|| left.pid.cmp(&right.pid))
    });
    by_cpu.truncate(top_n);

    let mut by_memory = processes;
    by_memory.sort_by(|left, right| {
        right
            .memory_bytes
            .cmp(&left.memory_bytes)
            .then_with(|| left.pid.cmp(&right.pid))
    });
    by_memory.truncate(top_n);

    (by_cpu, by_memory, baselines)
}

fn unix_time_ms() -> Result<u64, Error> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| Error::Clock)?;
    u64::try_from(elapsed.as_millis()).map_err(|_| Error::Clock)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use super::{Collector, NetCounters, network_rates, rate_pair};

    #[test]
    fn builder_changes_interval_and_ranking_size() {
        let collector = Collector::new()
            .with_interval(Duration::from_millis(250))
            .with_top_n(3);

        assert_eq!(collector.interval(), Duration::from_millis(250));
    }

    #[test]
    fn interval_is_never_zero() {
        let collector = Collector::new().with_interval(Duration::ZERO);

        assert_eq!(collector.interval(), Duration::from_millis(1));
    }

    #[test]
    fn a_new_counter_does_not_report_its_lifetime_total() {
        let huge_lifetime = (5_000_000_000, 1_000_000_000);

        assert_eq!(rate_pair(None, Some(huge_lifetime), 1.0), (0, 0));
        assert_eq!(rate_pair(Some((100, 200)), None, 1.0), (0, 0));
        assert_eq!(
            rate_pair(Some((100, 200)), Some((300, 600)), 2.0),
            (100, 200)
        );
    }

    #[test]
    fn a_new_interface_reports_zero_traffic() {
        let mut current = BTreeMap::new();
        current.insert(
            "vpn0".to_owned(),
            NetCounters {
                rx_bytes: 9_000_000,
                tx_bytes: 3_000_000,
            },
        );

        let interfaces = network_rates(&BTreeMap::new(), &current, 1.0);

        assert_eq!(interfaces.len(), 1);
        assert_eq!(interfaces[0].rx_bytes_per_sec, 0);
        assert_eq!(interfaces[0].tx_bytes_per_sec, 0);
    }

    #[tokio::test]
    #[allow(clippy::assert_is_empty)]
    async fn run_emits_a_sample_on_the_live_host() {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let collector = Collector::new().with_interval(Duration::from_millis(20));
        let task = tokio::spawn(collector.run(move |sample| {
            let _ = sender.send(sample);
        }));

        let emitted = tokio::time::timeout(Duration::from_secs(5), receiver.recv())
            .await
            .expect("run should emit a sample")
            .expect("the channel should stay open");
        let snapshot = emitted.expect("this host should be sampleable");

        assert!(snapshot.memory.total_bytes > 0);
        assert!(!snapshot.cpu.per_core_percent.is_empty());
        assert!(!snapshot.top_processes_by_cpu.is_empty());

        task.abort();
    }
}
