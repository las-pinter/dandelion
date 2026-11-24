//! Resource thresholds: CPU, memory, and disks that stay too full.

use dandelion_sysmon::Snapshot;

use super::Triggers;
use crate::notifications::Notice;

/// How full a resource must be.
const THRESHOLD_PERCENT: f32 = 90.0;
/// How long it must stay that full.
const SUSTAIN_MS: u64 = 120_000;

/// A resource that can stay above the threshold.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Resource {
    /// The processor.
    Cpu,
    /// Physical memory.
    Memory,
    /// One mounted filesystem, by mount point.
    Disk(String),
}

impl Resource {
    /// The browser dedupe tag for this resource.
    #[must_use]
    pub fn tag(&self) -> String {
        match self {
            Self::Cpu => "res:cpu".to_owned(),
            Self::Memory => "res:memory".to_owned(),
            Self::Disk(mount) => format!("res:disk:{mount}"),
        }
    }

    /// The phrase used in a notice body.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Cpu => "CPU".to_owned(),
            Self::Memory => "Memory".to_owned(),
            Self::Disk(mount) => format!("Disk {mount}"),
        }
    }
}

impl Triggers {
    /// Checks one metrics sample; the first notice arrives after the resource
    /// has stayed full for two minutes.
    pub fn observe_sample(&mut self, snapshot: &Snapshot, now_ms: u64) -> Vec<Notice> {
        let mut notices = Vec::new();
        let mut present = std::collections::HashSet::new();
        present.insert(Resource::Cpu);
        self.check(
            &Resource::Cpu,
            snapshot.cpu.usage_percent,
            now_ms,
            &mut notices,
        );
        present.insert(Resource::Memory);
        self.check(
            &Resource::Memory,
            percent(snapshot.memory.used_bytes, snapshot.memory.total_bytes),
            now_ms,
            &mut notices,
        );
        for disk in &snapshot.disks {
            let resource = Resource::Disk(disk.mount_point.clone());
            present.insert(resource.clone());
            self.check(
                &resource,
                percent(disk.used_bytes, disk.total_bytes),
                now_ms,
                &mut notices,
            );
        }
        // A disk that disappears from the sample is forgotten; a remount is a
        // new crossing, not the old one still high.
        self.above_since
            .retain(|resource, _| present.contains(resource));
        self.high_notified
            .retain(|resource| present.contains(resource));
        notices
    }

    /// Forgets every open window; used when the metrics feed stalls, so one
    /// stale sample cannot age into a "sustained high" notice.
    pub fn reset_resources(&mut self) {
        self.above_since.clear();
        self.high_notified.clear();
    }

    fn check(&mut self, resource: &Resource, value: f32, now_ms: u64, notices: &mut Vec<Notice>) {
        if value < THRESHOLD_PERCENT {
            self.above_since.remove(resource);
            self.high_notified.remove(resource);
            return;
        }
        if self.high_notified.contains(resource) {
            return;
        }
        let started = *self.above_since.entry(resource.clone()).or_insert(now_ms);
        if now_ms.saturating_sub(started) >= SUSTAIN_MS {
            self.high_notified.insert(resource.clone());
            notices.push(Notice::new(
                "Resource high",
                format!(
                    "{} has been at or above {THRESHOLD_PERCENT:.0}% for two minutes.",
                    resource.label()
                ),
                resource.tag(),
                "/dashboard",
            ));
        }
    }
}

// A percentage only needs a few significant digits, so the f64 round trip is
// well inside the precision the threshold cares about.
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
fn percent(used: u64, total: u64) -> f32 {
    if total == 0 {
        return 0.0;
    }
    (used as f64 * 100.0 / total as f64) as f32
}

#[cfg(test)]
mod tests {
    use dandelion_sysmon::{Cpu, DiskMount, Memory, Snapshot};

    use super::super::Triggers;

    fn snapshot(cpu: f32, memory_used: u64, disk_used: u64) -> Snapshot {
        Snapshot {
            timestamp_ms: 1,
            uptime_secs: 1.0,
            cpu: Cpu {
                usage_percent: cpu,
                ..Cpu::default()
            },
            memory: Memory {
                total_bytes: 100,
                used_bytes: memory_used,
                ..Memory::default()
            },
            disks: vec![DiskMount {
                mount_point: "/".to_owned(),
                total_bytes: 100,
                used_bytes: disk_used,
                ..DiskMount::default()
            }],
            ..Snapshot::default()
        }
    }

    #[test]
    fn a_high_resource_is_announced_after_two_minutes() {
        let mut triggers = Triggers::default();

        assert_eq!(triggers.observe_sample(&snapshot(95.0, 10, 10), 0).len(), 0);
        assert_eq!(
            triggers
                .observe_sample(&snapshot(95.0, 10, 10), 119_999)
                .len(),
            0
        );
        let notices = triggers.observe_sample(&snapshot(95.0, 10, 10), 120_000);
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].tag, "res:cpu");
        assert_eq!(
            triggers
                .observe_sample(&snapshot(95.0, 10, 10), 200_000)
                .len(),
            0
        );

        triggers.observe_sample(&snapshot(10.0, 10, 10), 201_000);
        assert_eq!(
            triggers
                .observe_sample(&snapshot(95.0, 10, 10), 300_000)
                .len(),
            0
        );
        assert_eq!(
            triggers
                .observe_sample(&snapshot(95.0, 10, 10), 420_000)
                .len(),
            1,
            "a new crossing is announced again"
        );
    }

    #[test]
    fn memory_and_disks_have_their_own_thresholds() {
        let mut triggers = Triggers::default();

        let notices = triggers.observe_sample(&snapshot(10.0, 95, 95), 0);
        assert_eq!(notices.len(), 0);
        let notices = triggers.observe_sample(&snapshot(10.0, 95, 95), 120_000);
        let tags: Vec<&str> = notices.iter().map(|notice| notice.tag.as_str()).collect();
        assert!(tags.contains(&"res:memory"));
        assert!(tags.contains(&"res:disk:/"));
    }

    #[test]
    fn a_stale_feed_resets_the_windows() {
        let mut triggers = Triggers::default();
        let high = snapshot(95.0, 10, 10);
        assert_eq!(triggers.observe_sample(&high, 0).len(), 0);
        assert_eq!(triggers.observe_sample(&high, 120_000).len(), 1);

        // A stalled collector: both the open window and the high mark must go.
        triggers.reset_resources();
        assert_eq!(
            triggers.observe_sample(&high, 140_000).len(),
            0,
            "the window starts again after a reset"
        );
        assert_eq!(
            triggers.observe_sample(&high, 260_000).len(),
            1,
            "the cleared high mark lets the same crossing notify again"
        );
    }

    #[test]
    fn a_vanished_disk_is_forgotten() {
        let mut triggers = Triggers::default();
        let high = snapshot(10.0, 10, 95);
        assert_eq!(triggers.observe_sample(&high, 0).len(), 0);
        assert_eq!(triggers.observe_sample(&high, 120_000).len(), 1);

        let mut gone = snapshot(10.0, 10, 0);
        gone.disks.clear();
        assert_eq!(triggers.observe_sample(&gone, 130_000).len(), 0);

        assert_eq!(
            triggers.observe_sample(&high, 140_000).len(),
            0,
            "the remounted disk starts its two minutes again"
        );
        assert_eq!(
            triggers.observe_sample(&high, 260_000).len(),
            1,
            "a remounted disk that stays high notifies again"
        );
    }
}
