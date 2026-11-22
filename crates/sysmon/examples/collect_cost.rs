//! Measures how much CPU the collector spends at a one second interval.
//!
//! ```sh
//! cargo run --release -p dandelion-sysmon --example collect_cost -- 30
//! ```

use std::time::{Duration, Instant};

use dandelion_sysmon::Collector;

#[allow(clippy::cast_precision_loss)]
fn main() {
    let samples: u64 = std::env::args()
        .nth(1)
        .and_then(|argument| argument.parse().ok())
        .unwrap_or(30);

    let mut collector = Collector::new();
    // Warm caches and establish the rate baseline before measuring.
    collector.sample().expect("warm-up sample");

    let started = Instant::now();
    let cpu_started = self_cpu_seconds();
    for _ in 0..samples {
        std::thread::sleep(Duration::from_secs(1));
        collector.sample().expect("sample");
    }
    let elapsed = started.elapsed().as_secs_f64();
    let cpu_seconds = self_cpu_seconds() - cpu_started;

    println!("samples: {samples} over {elapsed:.2} s");
    println!(
        "collector CPU: {cpu_seconds:.3} s = {:.3}% of one core ({:.0} us per sample)",
        cpu_seconds / elapsed * 100.0,
        cpu_seconds * 1e6 / samples as f64,
    );
}

/// User plus system CPU time of this process, in seconds.
#[allow(clippy::cast_precision_loss)]
fn self_cpu_seconds() -> f64 {
    let text = std::fs::read_to_string("/proc/self/stat").expect("read /proc/self/stat");
    let close = text.rfind(')').expect("/proc/self/stat has a command name");
    let fields: Vec<&str> = text[close + 1..].split_whitespace().collect();
    let utime: u64 = fields[11].parse().expect("utime");
    let stime: u64 = fields[12].parse().expect("stime");

    // /proc reports CPU time in USER_HZ ticks, which is 100 on Linux.
    (utime + stime) as f64 / 100.0
}
