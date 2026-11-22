//! Reduces a Docker statistics sample to the fields the UI needs.
//!
//! Docker reports cumulative counters, so CPU use is a delta between the
//! current sample and the previous one, scaled by the number of cores. The
//! formula matches `docker stats`.

use bollard::models::{ContainerCpuStats, ContainerStatsResponse};

use super::types::ContainerStats;

/// Builds our statistics type from one raw Docker sample.
pub(crate) fn compute_stats(container: &str, raw: &ContainerStatsResponse) -> ContainerStats {
    let (received_bytes, sent_bytes) = raw.networks.as_ref().map_or((0, 0), |interfaces| {
        interfaces
            .values()
            .fold((0_u64, 0_u64), |(received, sent), interface| {
                (
                    received.saturating_add(interface.rx_bytes.unwrap_or(0)),
                    sent.saturating_add(interface.tx_bytes.unwrap_or(0)),
                )
            })
    });
    ContainerStats {
        container: container.to_owned(),
        cpu_percent: cpu_percent(raw),
        memory_usage_bytes: raw.memory_stats.as_ref().and_then(|memory| memory.usage),
        memory_limit_bytes: raw.memory_stats.as_ref().and_then(|memory| memory.limit),
        network_rx_bytes: received_bytes,
        network_tx_bytes: sent_bytes,
        pids: raw.pids_stats.as_ref().and_then(|pids| pids.current),
    }
}

fn cpu_percent(raw: &ContainerStatsResponse) -> Option<f64> {
    let current = raw.cpu_stats.as_ref()?;
    let previous = raw.precpu_stats.as_ref()?;
    let total = current.cpu_usage.as_ref()?.total_usage?;
    let previous_total = previous.cpu_usage.as_ref()?.total_usage?;
    let system = current.system_cpu_usage?;
    let previous_system = previous.system_cpu_usage?;
    let cpu_delta = total.checked_sub(previous_total)?;
    let system_delta = system.checked_sub(previous_system)?;
    if cpu_delta == 0 || system_delta == 0 {
        return Some(0.0);
    }
    let cores = core_count(current);
    #[allow(clippy::cast_precision_loss)]
    Some((cpu_delta as f64 / system_delta as f64) * f64::from(cores) * 100.0)
}

fn core_count(current: &ContainerCpuStats) -> u32 {
    current
        .online_cpus
        .filter(|cores| *cores > 0)
        .or_else(|| {
            current
                .cpu_usage
                .as_ref()
                .and_then(|usage| usage.percpu_usage.as_ref())
                .and_then(|cores| u32::try_from(cores.len()).ok())
                .filter(|cores| *cores > 0)
        })
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::compute_stats;
    use crate::docker::types::ContainerStats;
    use bollard::models::{
        ContainerCpuStats, ContainerCpuUsage, ContainerMemoryStats, ContainerNetworkStats,
        ContainerPidsStats, ContainerStatsResponse,
    };
    use std::collections::HashMap;

    fn cpu(total: u64, system: u64, cores: u32) -> ContainerCpuStats {
        ContainerCpuStats {
            cpu_usage: Some(ContainerCpuUsage {
                total_usage: Some(total),
                ..ContainerCpuUsage::default()
            }),
            system_cpu_usage: Some(system),
            online_cpus: Some(cores),
            ..ContainerCpuStats::default()
        }
    }

    fn sample(current: ContainerCpuStats, previous: ContainerCpuStats) -> ContainerStatsResponse {
        ContainerStatsResponse {
            cpu_stats: Some(current),
            precpu_stats: Some(previous),
            ..ContainerStatsResponse::default()
        }
    }

    #[test]
    fn cpu_percent_is_the_delta_share_of_one_core() {
        let raw = sample(
            cpu(2_000_000_000, 4_000_000_000, 1),
            cpu(1_000_000_000, 2_000_000_000, 1),
        );

        let stats = compute_stats("probe", &raw);

        assert_eq!(stats.cpu_percent, Some(50.0));
    }

    #[test]
    fn cpu_percent_scales_with_the_core_count() {
        let raw = sample(
            cpu(2_000_000_000, 2_000_000_000, 2),
            cpu(1_000_000_000, 1_000_000_000, 2),
        );

        let stats = compute_stats("probe", &raw);

        assert_eq!(stats.cpu_percent, Some(200.0));
    }

    #[test]
    fn a_zero_delta_is_zero_percent() {
        let raw = sample(cpu(1_000, 1_000, 1), cpu(1_000, 1_000, 1));

        assert_eq!(compute_stats("probe", &raw).cpu_percent, Some(0.0));
    }

    #[test]
    fn a_missing_previous_sample_has_no_cpu_percent() {
        let mut raw = sample(cpu(1_000, 1_000, 1), cpu(0, 0, 1));
        raw.precpu_stats = None;

        assert_eq!(compute_stats("probe", &raw).cpu_percent, None);
    }

    #[test]
    fn counters_that_went_backwards_have_no_cpu_percent() {
        let raw = sample(cpu(1_000, 2_000, 1), cpu(2_000, 4_000, 1));

        assert_eq!(compute_stats("probe", &raw).cpu_percent, None);
    }

    #[test]
    fn memory_network_and_pids_are_reported() {
        let raw = ContainerStatsResponse {
            memory_stats: Some(ContainerMemoryStats {
                usage: Some(1_048_576),
                limit: Some(2_097_152),
                ..ContainerMemoryStats::default()
            }),
            networks: Some(HashMap::from([
                (
                    "eth0".to_owned(),
                    ContainerNetworkStats {
                        rx_bytes: Some(100),
                        tx_bytes: Some(200),
                        ..ContainerNetworkStats::default()
                    },
                ),
                (
                    "eth1".to_owned(),
                    ContainerNetworkStats {
                        rx_bytes: Some(50),
                        tx_bytes: Some(25),
                        ..ContainerNetworkStats::default()
                    },
                ),
            ])),
            pids_stats: Some(ContainerPidsStats {
                current: Some(3),
                limit: None,
            }),
            ..ContainerStatsResponse::default()
        };

        let stats = compute_stats("probe", &raw);

        assert_eq!(
            stats,
            ContainerStats {
                container: "probe".to_owned(),
                cpu_percent: None,
                memory_usage_bytes: Some(1_048_576),
                memory_limit_bytes: Some(2_097_152),
                network_rx_bytes: 150,
                network_tx_bytes: 225,
                pids: Some(3),
            }
        );
    }

    #[test]
    fn an_empty_sample_still_returns_the_container_name() {
        let stats = compute_stats("probe", &ContainerStatsResponse::default());

        assert_eq!(stats.container, "probe");
        assert_eq!(stats.cpu_percent, None);
        assert_eq!(stats.memory_usage_bytes, None);
        assert_eq!(stats.network_rx_bytes, 0);
        assert_eq!(stats.network_tx_bytes, 0);
        assert_eq!(stats.pids, None);
    }
}
