//! One real sample, checked against the host's own procfs.

use std::process::Command;

use dandelion_sysmon::Collector;

fn host_mem_total_bytes() -> u64 {
    let text = std::fs::read_to_string("/proc/meminfo").expect("read /proc/meminfo");
    let line = text
        .lines()
        .find(|line| line.starts_with("MemTotal:"))
        .expect("MemTotal line");

    let kibibytes: u64 = line
        .split_whitespace()
        .nth(1)
        .expect("MemTotal value")
        .parse()
        .expect("numeric MemTotal");

    kibibytes * 1024
}

fn host_core_count() -> usize {
    let output = Command::new("nproc").output().expect("run nproc");
    String::from_utf8(output.stdout)
        .expect("nproc prints utf-8")
        .trim()
        .parse()
        .expect("numeric nproc")
}

#[test]
#[allow(clippy::assert_is_empty)]
fn one_live_sample_is_plausible_for_this_host() {
    let mut collector = Collector::new();

    let snapshot = collector.sample().expect("sample this host");

    assert_eq!(
        snapshot.cpu.per_core_percent.len(),
        host_core_count(),
        "per-core usage should have one entry per CPU"
    );
    assert_eq!(
        snapshot.memory.total_bytes,
        host_mem_total_bytes(),
        "total memory should match /proc/meminfo"
    );
    assert!(snapshot.memory.used_bytes <= snapshot.memory.total_bytes);
    assert!(snapshot.memory.available_bytes <= snapshot.memory.total_bytes);
    assert!(snapshot.uptime_secs > 0.0);
    assert!(
        snapshot
            .cpu
            .per_core_percent
            .iter()
            .all(|percent| (0.0..=100.0).contains(percent))
    );

    let root = snapshot
        .disks
        .iter()
        .find(|disk| disk.mount_point == "/")
        .expect("the root filesystem is mounted");
    assert!(root.total_bytes > 0);
    assert!(root.used_bytes <= root.total_bytes);

    assert!(!snapshot.top_processes_by_cpu.is_empty());
    assert!(!snapshot.top_processes_by_memory.is_empty());
    assert!(
        snapshot
            .top_processes_by_cpu
            .windows(2)
            .all(|pair| pair[0].cpu_percent >= pair[1].cpu_percent)
    );

    // The first sample has no previous counters to diff against.
    assert!(
        snapshot
            .network
            .iter()
            .all(|interface| interface.rx_bytes_per_sec == 0 && interface.tx_bytes_per_sec == 0)
    );
}
