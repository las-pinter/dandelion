//! Parsers for `/proc/loadavg` and `/proc/uptime`.

use crate::types::LoadAverage;

/// Parses the three load averages from `/proc/loadavg`.
pub(crate) fn parse_load(text: &str) -> Option<LoadAverage> {
    let mut fields = text.split_whitespace();

    Some(LoadAverage {
        one: fields.next()?.parse().ok()?,
        five: fields.next()?.parse().ok()?,
        fifteen: fields.next()?.parse().ok()?,
    })
}

/// Parses the uptime in seconds from `/proc/uptime`.
pub(crate) fn parse_uptime(text: &str) -> Option<f64> {
    text.split_whitespace().next()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::{parse_load, parse_uptime};

    #[test]
    fn parses_load_averages() {
        let load = parse_load("1.20 2.13 2.17 1/900 3061164\n").expect("parse load");

        assert_eq!(load.one, 1.20);
        assert_eq!(load.five, 2.13);
        assert_eq!(load.fifteen, 2.17);
    }

    #[test]
    fn rejects_incomplete_load() {
        assert!(parse_load("1.20 2.13\n").is_none());
    }

    #[test]
    fn parses_uptime_seconds() {
        assert_eq!(parse_uptime("104175.73 991091.89\n"), Some(104_175.73));
    }

    #[test]
    fn rejects_non_numeric_uptime() {
        assert!(parse_uptime("soon\n").is_none());
    }
}
