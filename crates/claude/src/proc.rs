//! Process checks backed by `/proc`.
//!
//! A session file can outlive a crashed process, and process ids are reused.
//! The session record's `procStart` is compared with field 22 of
//! `/proc/<pid>/stat` before a session is trusted.

use std::path::Path;

/// Reads the process start time from `/proc/<pid>/stat`, or `None` when the
/// process does not exist.
#[must_use]
pub fn process_start_time(proc_root: &Path, pid: u32) -> Option<String> {
    let stat = std::fs::read_to_string(proc_root.join(pid.to_string()).join("stat")).ok()?;
    parse_stat_start_time(&stat)
}

/// Whether `/proc/<pid>/stat` start time equals the value recorded in the
/// session file.
#[must_use]
pub fn process_start_matches(proc_root: &Path, pid: u32, expected: &str) -> bool {
    process_start_time(proc_root, pid).is_some_and(|actual| actual == expected.trim())
}

/// Reads `HC_TERMINAL_ID` from `/proc/<pid>/environ`.
///
/// Only that variable is extracted. Other values in the environment are never
/// parsed, kept, or logged.
#[must_use]
pub fn terminal_id(proc_root: &Path, pid: u32) -> Option<String> {
    let environ = std::fs::read(proc_root.join(pid.to_string()).join("environ")).ok()?;
    parse_environ_var(&environ, "HC_TERMINAL_ID").filter(|value| !value.is_empty())
}

/// Parses field 22 (start time in clock ticks) from the content of a `stat`
/// file.
///
/// The second field is the executable name in parentheses and may itself
/// contain spaces or parentheses, so fields are counted from the last `)`.
#[must_use]
pub fn parse_stat_start_time(content: &str) -> Option<String> {
    let tail = content.rsplit_once(')')?.1;
    // `state` is field 3 and is the first token after `comm`; starttime is
    // field 22, the twentieth token.
    let start_time = tail.split_whitespace().nth(19)?;
    Some(start_time.to_owned())
}

fn parse_environ_var(environ: &[u8], key: &str) -> Option<String> {
    for entry in environ.split(|byte| *byte == 0) {
        let Ok(entry) = std::str::from_utf8(entry) else {
            continue;
        };
        if let Some(value) = entry
            .strip_prefix(key)
            .and_then(|rest| rest.strip_prefix('='))
        {
            return Some(value.to_owned());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{parse_environ_var, parse_stat_start_time};

    #[test]
    fn reads_start_time_after_a_plain_command_name() {
        let stat = "424242 (claude) R 1 424242 424242 0 -1 4194304 100 0 0 0 1 2 0 0 20 0 5 0 12345 100000 123";

        assert_eq!(parse_stat_start_time(stat).as_deref(), Some("12345"));
    }

    #[test]
    fn reads_start_time_when_the_command_name_contains_spaces_and_parentheses() {
        let stat = "424242 (claude work (x)) S 1 424242 424242 0 -1 4194304 100 0 0 0 1 2 0 0 20 0 5 0 98765 100000 123";

        assert_eq!(parse_stat_start_time(stat).as_deref(), Some("98765"));
    }

    #[test]
    fn missing_start_time_is_none() {
        assert_eq!(parse_stat_start_time("not a stat line"), None);
    }

    #[test]
    fn extracts_only_the_requested_variable() {
        let environ = b"PATH=/bin\0HC_TERMINAL_ID=t42\0OTHER=secret\0";

        assert_eq!(
            parse_environ_var(environ, "HC_TERMINAL_ID").as_deref(),
            Some("t42")
        );
        assert_eq!(parse_environ_var(environ, "HC_SCOPE"), None);
    }

    #[test]
    fn ignores_a_key_that_is_only_a_prefix() {
        let environ = b"HC_TERMINAL_ID_EXTRA=t99\0";

        assert_eq!(parse_environ_var(environ, "HC_TERMINAL_ID"), None);
    }
}
