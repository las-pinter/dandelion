//! Parsing of the `/proc` files the process table needs.
//!
//! The parsers take raw text so they can be tested without a real kernel
//! process, including the awkward cases: a `comm` that contains spaces and
//! parentheses, an empty command line for a kernel thread, and a UID with no
//! passwd entry.

use std::collections::HashMap;

use super::{ProcessError, ProcessState};

/// The `/proc/<pid>/stat` fields used by the process table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProcStat {
    pub comm: String,
    pub state: ProcessState,
    pub ppid: u32,
    pub utime_ticks: u64,
    pub stime_ticks: u64,
    pub nice: i32,
    pub start_ticks: u64,
    pub rss_pages: i64,
}

const STAT: &str = "stat";

const fn malformed(reason: &'static str) -> ProcessError {
    ProcessError::Malformed {
        source_file: STAT,
        reason,
    }
}

/// Parses `/proc/<pid>/stat`. Everything after the executable name is a
/// whitespace-separated list of numbers; the name itself may contain spaces and
/// parentheses, so it is delimited by the first `(` and the last `)`.
pub(crate) fn parse_stat(raw: &str) -> Result<ProcStat, ProcessError> {
    let open = raw
        .find('(')
        .ok_or_else(|| malformed("missing opening parenthesis"))?;
    let close = raw
        .rfind(')')
        .ok_or_else(|| malformed("missing closing parenthesis"))?;
    if close <= open {
        return Err(malformed("misordered parentheses"));
    }
    let comm = raw[open + 1..close].to_owned();
    let fields: Vec<&str> = raw[close + 1..].split_whitespace().collect();
    let state_letter = fields
        .first()
        .and_then(|field| field.chars().next())
        .ok_or_else(|| malformed("missing state"))?;
    Ok(ProcStat {
        comm,
        state: ProcessState::from_letter(state_letter),
        ppid: field(&fields, 1)?,
        utime_ticks: field(&fields, 11)?,
        stime_ticks: field(&fields, 12)?,
        nice: field(&fields, 16)?,
        start_ticks: field(&fields, 19)?,
        rss_pages: field(&fields, 21)?,
    })
}

fn field<T>(fields: &[&str], index: usize) -> Result<T, ProcessError>
where
    T: std::str::FromStr,
{
    let raw = fields
        .get(index)
        .ok_or_else(|| malformed("missing numeric field"))?;
    raw.parse()
        .map_err(|_| malformed("unparsable numeric field"))
}

/// Parses `/etc/passwd` into a uid-to-name map. Malformed lines are skipped so
/// that one bad entry cannot hide every process owner.
pub(crate) fn parse_passwd(raw: &str) -> HashMap<u32, String> {
    raw.lines()
        .filter_map(|line| {
            let mut parts = line.split(':');
            let name = parts.next()?;
            parts.next()?;
            let uid = parts.next()?.parse::<u32>().ok()?;
            Some((uid, name.to_owned()))
        })
        .collect()
}

/// Finds the boot time (seconds since the epoch) in `/proc/stat`.
pub(crate) fn parse_btime(raw: &str) -> Option<i64> {
    raw.lines().find_map(|line| {
        let rest = line.strip_prefix("btime ")?;
        rest.trim().parse::<i64>().ok()
    })
}

/// Finds the real uid in `/proc/<pid>/status`.
pub(crate) fn parse_status_uid(raw: &str) -> Option<u32> {
    raw.lines().find_map(|line| {
        let rest = line.strip_prefix("Uid:")?;
        rest.split_whitespace().next()?.parse::<u32>().ok()
    })
}

/// Joins `/proc/<pid>/cmdline` with spaces. Empty for kernel threads, whose
/// command line is a single zero byte.
pub(crate) fn parse_cmdline(raw: &[u8]) -> Option<String> {
    let args: Vec<String> = raw
        .split(|byte| *byte == 0)
        .filter(|chunk| !chunk.is_empty())
        .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
        .collect();
    if args.is_empty() {
        None
    } else {
        Some(args.join(" "))
    }
}

/// User-space clock ticks per second (`USER_HZ`).
pub(crate) fn clock_ticks_per_second() -> u64 {
    let ticks = rustix::param::clock_ticks_per_second();
    if ticks == 0 { 100 } else { ticks }
}

/// The kernel page size in bytes.
pub(crate) fn page_size() -> u64 {
    u64::try_from(rustix::param::page_size()).unwrap_or(4096)
}

#[cfg(test)]
mod tests {
    use super::{parse_btime, parse_cmdline, parse_passwd, parse_stat, parse_status_uid};
    use crate::process::{ProcessError, ProcessState};

    const SAMPLE_STAT: &str =
        "42 (sleep) S 7 42 42 0 -1 4194560 123 0 0 0 5 7 0 0 20 0 1 0 12345 4096 100";

    #[test]
    fn stat_parses_every_field_the_table_needs() {
        let stat = parse_stat(SAMPLE_STAT).expect("sample stat must parse");

        assert_eq!(stat.comm, "sleep");
        assert_eq!(stat.state, ProcessState::Sleeping);
        assert_eq!(stat.ppid, 7);
        assert_eq!(stat.utime_ticks, 5);
        assert_eq!(stat.stime_ticks, 7);
        assert_eq!(stat.nice, 0);
        assert_eq!(stat.start_ticks, 12_345);
        assert_eq!(stat.rss_pages, 100);
    }

    #[test]
    fn stat_keeps_spaces_and_parentheses_in_the_name() {
        let raw = "77 (web (worker)) R 1 77 77 0 -1 0 0 0 0 0 1 2 0 0 20 0 1 0 9 10 11";

        let stat = parse_stat(raw).expect("name with parentheses must parse");

        assert_eq!(stat.comm, "web (worker)");
        assert_eq!(stat.state, ProcessState::Running);
    }

    #[test]
    fn stat_rejects_truncated_and_unparsable_input() {
        for raw in [
            "42 sleep S 7",
            "42 (sleep) S 7 42",
            "42 (sleep)",
            "42 (sleep) S 7 42 42 0 -1 4194560 123 0 0 0 bad 7 0 0 20 0 1 0 12345 4096 100",
        ] {
            let err = parse_stat(raw).expect_err("truncated stat must fail");
            assert!(matches!(err, ProcessError::Malformed { .. }));
        }
    }

    #[test]
    fn passwd_maps_uids_and_skips_malformed_lines() {
        let raw = "root:x:0:0:root:/root:/bin/bash\n\
                   probe:x:1000:1000:Probe user:/srv/probe:/bin/sh\n\
                   broken-line\n\
                   baduid:x:notanumber:3::/:/bin/sh\n";

        let users = parse_passwd(raw);

        assert_eq!(users.get(&0).map(String::as_str), Some("root"));
        assert_eq!(users.get(&1000).map(String::as_str), Some("probe"));
        assert_eq!(users.len(), 2);
    }

    #[test]
    fn btime_is_read_from_proc_stat() {
        let raw = "cpu  1 2 3 4\nbtime 1700000000\nprocesses 3\n";

        assert_eq!(parse_btime(raw), Some(1_700_000_000));
        assert_eq!(parse_btime("cpu 1 2 3 4\n"), None);
    }

    #[test]
    fn status_uid_takes_the_real_uid() {
        let raw = "Name:\tsleep\nUid:\t1000\t1000\t1000\t1000\nGid:\t1000\t1000\n";

        assert_eq!(parse_status_uid(raw), Some(1000));
        assert_eq!(parse_status_uid("Name:\tsleep\n"), None);
    }

    #[test]
    fn cmdline_joins_arguments_and_survives_non_utf8() {
        assert_eq!(
            parse_cmdline(b"/usr/bin/sleep\x0030\x00"),
            Some("/usr/bin/sleep 30".to_owned())
        );
        assert_eq!(parse_cmdline(b"\x00"), None);
        assert_eq!(
            parse_cmdline(b"/bin/echo\x00\xff\x00"),
            Some("/bin/echo \u{fffd}".to_owned())
        );
    }
}
