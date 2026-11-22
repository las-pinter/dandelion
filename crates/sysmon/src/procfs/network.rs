//! Parser for `/proc/net/dev`.

use std::collections::BTreeMap;

/// Cumulative traffic counters of one network interface.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct NetCounters {
    pub(crate) rx_bytes: u64,
    pub(crate) tx_bytes: u64,
}

/// Parses received and transmitted bytes per interface.
///
/// Returns `None` when no interface line is valid.
pub(crate) fn parse(text: &str) -> Option<BTreeMap<String, NetCounters>> {
    let mut interfaces = BTreeMap::new();

    for line in text.lines() {
        let Some((name, rest)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() {
            continue;
        }

        let mut fields = rest.split_whitespace();
        let Some(rx_bytes) = fields.next().and_then(|value| value.parse().ok()) else {
            continue;
        };
        // The transmit byte counter is the ninth value of the row.
        let Some(tx_bytes) = fields.nth(7).and_then(|value| value.parse().ok()) else {
            continue;
        };

        interfaces.insert(name.to_owned(), NetCounters { rx_bytes, tx_bytes });
    }

    if interfaces.is_empty() {
        None
    } else {
        Some(interfaces)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{NetCounters, parse};

    const NET_DEV: &str = "\
Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
    lo: 6768575   84877    0    0    0     0          0         0  6768575   84877    0    0    0     0       0          0
enp0s3: 1661257409 6182981    0    0    0     0          0         0 2907719120 5329297    0   10    0     0       0          0
";

    #[test]
    fn parses_each_interface() {
        let parsed = parse(NET_DEV).expect("parse net/dev");

        assert_eq!(
            parsed,
            BTreeMap::from([
                (
                    "lo".to_owned(),
                    NetCounters {
                        rx_bytes: 6_768_575,
                        tx_bytes: 6_768_575,
                    },
                ),
                (
                    "enp0s3".to_owned(),
                    NetCounters {
                        rx_bytes: 1_661_257_409,
                        tx_bytes: 2_907_719_120,
                    },
                ),
            ])
        );
    }

    #[test]
    fn rejects_a_truncated_row() {
        assert!(parse("eth0: 10 1 0 0 0 0 0 0\n").is_none());
    }

    #[test]
    fn rejects_the_header_only() {
        assert!(parse("Inter-| Receive | Transmit\n").is_none());
    }
}
