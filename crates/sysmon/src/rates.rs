//! Delta maths that turns cumulative counters into rates.
//!
//! Every function takes the elapsed time as an argument. Nothing here reads a
//! clock, which keeps the arithmetic testable with injected samples.

use crate::procfs::CpuCounters;

/// Clock ticks the kernel reports `/proc` times in (`USER_HZ`).
const USER_HZ: f64 = 100.0;

/// CPU usage between two samples of the same core, in percent.
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
pub(crate) fn cpu_percent(previous: CpuCounters, current: CpuCounters) -> f32 {
    let total = current.total.saturating_sub(previous.total);
    if total == 0 {
        return 0.0;
    }

    let idle = current.idle.saturating_sub(previous.idle).min(total);
    let busy = total - idle;
    let percent = (busy as f64 / total as f64) * 100.0;

    (percent as f32).clamp(0.0, 100.0)
}

/// Per-core CPU usage between two samples.
///
/// Cores that appeared since the previous sample report zero.
#[must_use]
pub(crate) fn per_core_percent(previous: &[CpuCounters], current: &[CpuCounters]) -> Vec<f32> {
    current
        .iter()
        .enumerate()
        .map(|(index, counters)| {
            previous
                .get(index)
                .map_or(0.0, |before| cpu_percent(*before, *counters))
        })
        .collect()
}

/// Byte rate between two counter readings.
///
/// A counter that moved backwards was reset and reports zero.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]
pub(crate) fn bytes_per_sec(previous: u64, current: u64, elapsed_secs: f64) -> u64 {
    if current <= previous || elapsed_secs <= 0.0 {
        return 0;
    }

    let delta = (current - previous) as f64;
    (delta / elapsed_secs).round() as u64
}

/// Process CPU usage between two samples, in percent of one core.
///
/// `previous_jiffies` is `None` for a process that was not present in the
/// previous sample, which reports zero rather than a bogus lifetime average.
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
pub(crate) fn process_percent(
    previous_jiffies: Option<u64>,
    current_jiffies: u64,
    elapsed_secs: f64,
) -> f32 {
    let Some(previous_jiffies) = previous_jiffies else {
        return 0.0;
    };
    if elapsed_secs <= 0.0 {
        return 0.0;
    }

    let delta = current_jiffies.saturating_sub(previous_jiffies);
    let percent = (delta as f64 / USER_HZ / elapsed_secs) * 100.0;

    percent as f32
}

#[cfg(test)]
mod tests {
    use super::{bytes_per_sec, cpu_percent, per_core_percent, process_percent};
    use crate::procfs::CpuCounters;

    fn counters(total: u64, idle: u64) -> CpuCounters {
        CpuCounters { total, idle }
    }

    #[test]
    fn cpu_percent_uses_only_the_delta() {
        let previous = counters(1_000, 500);
        let current = counters(2_000, 1_300);

        // 1000 new jiffies, 800 of them idle.
        assert_eq!(cpu_percent(previous, current), 20.0);
    }

    #[test]
    fn cpu_percent_is_zero_without_a_delta() {
        assert_eq!(cpu_percent(counters(500, 100), counters(500, 100)), 0.0);
    }

    #[test]
    fn cpu_percent_clamps_a_reset() {
        let percent = cpu_percent(counters(2_000, 1_500), counters(10, 1));

        assert_eq!(percent, 0.0);
    }

    #[test]
    fn per_core_handles_a_new_core() {
        let previous = [counters(200, 100)];
        let current = [counters(200, 100), counters(300, 150)];

        assert_eq!(per_core_percent(&previous, &current), vec![0.0, 0.0]);
    }

    #[test]
    fn byte_rate_divides_by_elapsed_seconds() {
        assert_eq!(bytes_per_sec(1_000, 3_000, 2.0), 1_000);
    }

    #[test]
    fn byte_rate_rounds_to_the_nearest_byte() {
        assert_eq!(bytes_per_sec(0, 1_500, 2.0), 750);
        assert_eq!(bytes_per_sec(0, 3, 2.0), 2);
    }

    #[test]
    fn byte_rate_is_zero_after_a_counter_reset() {
        assert_eq!(bytes_per_sec(9_000, 100, 1.0), 0);
    }

    #[test]
    fn byte_rate_is_zero_without_elapsed_time() {
        assert_eq!(bytes_per_sec(0, 5_000, 0.0), 0);
    }

    #[test]
    fn process_percent_converts_jiffies_over_elapsed_time() {
        // 50 ticks in one second is half a core.
        assert_eq!(process_percent(Some(100), 150, 1.0), 50.0);
        // 100 ticks in half a second is two cores.
        assert_eq!(process_percent(Some(0), 100, 0.5), 200.0);
    }

    #[test]
    fn process_percent_is_zero_for_a_new_or_reset_process() {
        assert_eq!(process_percent(None, 100, 1.0), 0.0);
        assert_eq!(process_percent(Some(900), 5, 1.0), 0.0);
    }
}
