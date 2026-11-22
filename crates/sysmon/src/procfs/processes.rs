//! Parser for `/proc/<pid>/stat`.

/// The fields the collector needs from one process's `stat` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProcessCounters {
    pub(crate) pid: u32,
    pub(crate) name: String,
    /// Kernel start time in clock ticks since boot; identifies a reused PID.
    pub(crate) start_time: u64,
    /// User plus system time in clock ticks.
    pub(crate) cpu_jiffies: u64,
    /// Resident set size in pages.
    pub(crate) rss_pages: u64,
}

/// Parses one `/proc/<pid>/stat` line.
///
/// The command name is enclosed in parentheses and may itself contain spaces
/// or parentheses, so the parser anchors on the last closing parenthesis.
pub(crate) fn parse(text: &str) -> Option<ProcessCounters> {
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    if close <= open {
        return None;
    }

    let pid = text.get(..open)?.trim().parse().ok()?;
    let name = text.get(open + 1..close)?.to_owned();
    let rest = text.get(close + 1..)?;
    let fields: Vec<&str> = rest.split_whitespace().collect();

    // After the command name the fields are numbered from 3 (state), so the
    // utime field 14 is index 11, starttime field 22 is index 19, and rss
    // field 24 is index 21.
    let utime = fields.get(11)?.parse::<u64>().ok()?;
    let stime = fields.get(12)?.parse::<u64>().ok()?;
    let start_time = fields.get(19)?.parse::<u64>().ok()?;
    let rss_pages = fields.get(21)?.parse::<u64>().ok()?;

    Some(ProcessCounters {
        pid,
        name,
        start_time,
        cpu_jiffies: utime.saturating_add(stime),
        rss_pages,
    })
}

#[cfg(test)]
mod tests {
    use super::{ProcessCounters, parse};

    const SYSTEMD: &str = "1 (systemd) S 0 1 1 0 -1 4194560 239167 2187450 951 1790 411 394 10545 23771 20 0 1 0 6 24264704 2811 18446744073709551615 1 1 0 0 0 0 671173123 4096 1260 0 0 0 17 5 0 0 0 0 0 0 0 0 0 0 0 0 0";

    #[test]
    fn parses_pid_name_and_counters() {
        let parsed = parse(SYSTEMD).expect("parse stat");

        assert_eq!(
            parsed,
            ProcessCounters {
                pid: 1,
                name: "systemd".to_owned(),
                start_time: 6,
                cpu_jiffies: 411 + 394,
                rss_pages: 2811,
            }
        );
    }

    #[test]
    fn keeps_parentheses_inside_the_command_name() {
        let text = "42 (my (odd) process) S 1 2 3 0 -1 0 0 0 0 0 7 8 0 0 20 0 1 0 99 0 77 0";

        let parsed = parse(text).expect("parse stat");

        assert_eq!(parsed.name, "my (odd) process");
        assert_eq!(parsed.pid, 42);
        assert_eq!(parsed.cpu_jiffies, 15);
        assert_eq!(parsed.rss_pages, 77);
        assert_eq!(parsed.start_time, 99);
    }

    #[test]
    fn rejects_truncated_input() {
        assert!(parse("42 (short) S 1 2").is_none());
    }

    #[test]
    fn rejects_input_without_a_command_name() {
        assert!(parse("42 short S 1 2 3").is_none());
    }
}
