//! Parser for `/proc/meminfo`.

/// Memory counters read from `/proc/meminfo`, in bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct MemInfo {
    pub(crate) total: u64,
    pub(crate) available: u64,
    pub(crate) cache: u64,
    pub(crate) swap_total: u64,
    pub(crate) swap_free: u64,
}

/// Parses the fields the collector needs from `/proc/meminfo`.
///
/// Returns `None` when `MemTotal` is missing or a needed value is malformed.
pub(crate) fn parse(text: &str) -> Option<MemInfo> {
    let mut total_kb = None;
    let mut free_kb = None;
    let mut available_kb = None;
    let mut buffers_kb = None;
    let mut cached_kb = None;
    let mut sreclaimable_kb = None;
    let mut shmem_kb = None;
    let mut swap_total_kb = None;
    let mut swap_free_kb = None;

    for line in text.lines() {
        let Some((key, rest)) = line.split_once(':') else {
            continue;
        };
        let Some(value) = rest.split_whitespace().next() else {
            continue;
        };
        let Ok(value) = value.parse::<u64>() else {
            continue;
        };

        match key {
            "MemTotal" => total_kb = Some(value),
            "MemFree" => free_kb = Some(value),
            "MemAvailable" => available_kb = Some(value),
            "Buffers" => buffers_kb = Some(value),
            "Cached" => cached_kb = Some(value),
            "SReclaimable" => sreclaimable_kb = Some(value),
            "Shmem" => shmem_kb = Some(value),
            "SwapTotal" => swap_total_kb = Some(value),
            "SwapFree" => swap_free_kb = Some(value),
            _ => {}
        }
    }

    let total_kb = total_kb?;
    let available_kb = available_kb.or_else(|| {
        // Kernels before 3.14 do not report MemAvailable.
        Some(
            free_kb?
                + buffers_kb.unwrap_or(0)
                + cached_kb.unwrap_or(0)
                + sreclaimable_kb.unwrap_or(0),
        )
    });
    let available_kb = available_kb.unwrap_or(total_kb);

    Some(MemInfo {
        total: to_bytes(total_kb),
        available: to_bytes(available_kb),
        cache: to_bytes(
            cached_kb
                .unwrap_or(0)
                .saturating_add(sreclaimable_kb.unwrap_or(0))
                .saturating_sub(shmem_kb.unwrap_or(0)),
        ),
        swap_total: to_bytes(swap_total_kb.unwrap_or(0)),
        swap_free: to_bytes(swap_free_kb.unwrap_or(0)),
    })
}

fn to_bytes(kibibytes: u64) -> u64 {
    kibibytes.saturating_mul(1024)
}

#[cfg(test)]
mod tests {
    use super::{MemInfo, parse};

    const MEMINFO: &str = "\
MemTotal:       12673096 kB
MemFree:         1369424 kB
MemAvailable:    9790784 kB
Buffers:          619240 kB
Cached:          7515776 kB
SwapCached:            0 kB
Shmem:               504 kB
SReclaimable:     608232 kB
SwapTotal:       4194300 kB
SwapFree:        3891380 kB
";

    #[test]
    fn converts_kibibytes_to_bytes() {
        let parsed = parse(MEMINFO).expect("parse meminfo");

        assert_eq!(
            parsed,
            MemInfo {
                total: 12_673_096 * 1024,
                available: 9_790_784 * 1024,
                cache: (7_515_776 + 608_232 - 504) * 1024,
                swap_total: 4_194_300 * 1024,
                swap_free: 3_891_380 * 1024,
            }
        );
    }

    #[test]
    fn falls_back_to_free_buffers_cache_and_slab() {
        let text =
            "MemTotal: 100 kB\nMemFree: 20 kB\nBuffers: 5 kB\nCached: 15 kB\nSReclaimable: 10 kB\n";

        let parsed = parse(text).expect("parse meminfo");

        assert_eq!(parsed.available, 50 * 1024);
        assert_eq!(parsed.swap_total, 0);
    }

    #[test]
    fn rejects_missing_total() {
        assert!(parse("MemFree: 20 kB\n").is_none());
    }

    #[test]
    fn cache_never_underflows() {
        let text = "MemTotal: 100 kB\nMemAvailable: 50 kB\nCached: 1 kB\nShmem: 9 kB\n";

        let parsed = parse(text).expect("parse meminfo");

        assert_eq!(parsed.cache, 0);
    }
}
