//! Parser for `/proc/stat` CPU accounting.

/// Jiffies spent by one CPU or by all CPUs together.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CpuCounters {
    /// Total jiffies accounted, including idle time.
    pub(crate) total: u64,
    /// Jiffies spent idle or waiting for I/O.
    pub(crate) idle: u64,
}

/// The aggregate CPU line and one counter pair per core.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CpuTimes {
    /// Sum over all cores, from the `cpu` line.
    pub(crate) total: CpuCounters,
    /// Per-core counters, in kernel order.
    pub(crate) per_core: Vec<CpuCounters>,
}

/// Parses the `cpu` and `cpuN` lines of `/proc/stat`.
///
/// Returns `None` when the aggregate line is missing or malformed.
pub(crate) fn parse(text: &str) -> Option<CpuTimes> {
    let mut total = None;
    let mut per_core = Vec::new();

    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let Some(label) = fields.next() else {
            continue;
        };

        if label == "cpu" {
            total = Some(parse_counters(fields)?);
        } else if label
            .strip_prefix("cpu")
            .is_some_and(|index| index.parse::<usize>().is_ok())
        {
            per_core.push(parse_counters(fields)?);
        }
    }

    Some(CpuTimes {
        total: total?,
        per_core,
    })
}

/// Sums the first eight `/proc/stat` counters.
///
/// Fields are user, nice, system, idle, iowait, irq, softirq, and steal.
/// The guest counters are excluded because the kernel already accounts for
/// them in user and nice.
fn parse_counters<'a>(fields: impl Iterator<Item = &'a str>) -> Option<CpuCounters> {
    let mut values = [0_u64; 8];
    let mut seen = 0;

    for (slot, field) in values.iter_mut().zip(fields) {
        *slot = field.parse().ok()?;
        seen += 1;
    }

    if seen < 4 {
        return None;
    }

    let total = values.iter().sum();
    let idle = values[3].saturating_add(values[4]);

    Some(CpuCounters { total, idle })
}

#[cfg(test)]
mod tests {
    use super::{CpuCounters, parse};

    const STAT: &str = "\
cpu  100 20 30 400 50 5 6 7 8 9
cpu0 50 10 15 200 25 2 3 4 0 0
cpu1 50 10 15 200 25 3 3 3 0 0
intr 12345 0 0
ctxt 67890
";

    #[test]
    fn parses_aggregate_and_per_core_counters() {
        let parsed = parse(STAT).expect("parse /proc/stat");

        assert_eq!(parsed.total.total, 618);
        assert_eq!(parsed.total.idle, 450);
        assert_eq!(
            parsed.per_core,
            vec![
                CpuCounters {
                    total: 309,
                    idle: 225
                },
                CpuCounters {
                    total: 309,
                    idle: 225
                },
            ]
        );
    }

    #[test]
    fn guest_fields_do_not_inflate_the_total() {
        let parsed = parse("cpu  10 0 0 10 0 0 0 0 999 999\n").expect("parse");

        assert_eq!(parsed.total.total, 20);
        assert_eq!(parsed.total.idle, 10);
    }

    #[test]
    fn rejects_a_missing_aggregate_line() {
        assert!(parse("cpu0 1 2 3 4\n").is_none());
    }

    #[test]
    fn rejects_a_truncated_line() {
        assert!(parse("cpu  1 2 3\n").is_none());
    }
}
