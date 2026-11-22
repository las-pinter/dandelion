//! Snapshot types produced by the collector.

use serde::Serialize;
use utoipa::ToSchema;

/// One complete observation of the machine.
#[derive(Debug, Clone, Default, PartialEq, Serialize, ToSchema)]
pub struct Snapshot {
    /// Unix time of the sample in milliseconds.
    pub timestamp_ms: u64,
    /// Seconds since boot.
    pub uptime_secs: f64,
    /// Processor usage and load.
    pub cpu: Cpu,
    /// Physical and swap memory.
    pub memory: Memory,
    /// Mounted filesystems with their usage and I/O rates.
    pub disks: Vec<DiskMount>,
    /// Network interfaces with their traffic rates.
    pub network: Vec<NetworkInterface>,
    /// Temperature sensors that are available on the host.
    pub temperatures: Vec<Temperature>,
    /// The busiest processes, sorted by CPU usage, highest first.
    pub top_processes_by_cpu: Vec<Process>,
    /// The largest processes, sorted by resident memory, highest first.
    pub top_processes_by_memory: Vec<Process>,
}

/// Processor usage and load averages.
#[derive(Debug, Clone, Default, PartialEq, Serialize, ToSchema)]
pub struct Cpu {
    /// Usage across all cores in percent, from 0 to 100.
    pub usage_percent: f32,
    /// Usage of each core in percent, from 0 to 100.
    pub per_core_percent: Vec<f32>,
    /// One, five, and fifteen minute load averages.
    pub load: LoadAverage,
}

/// The kernel's one, five, and fifteen minute load averages.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, ToSchema)]
pub struct LoadAverage {
    /// One minute load average.
    pub one: f32,
    /// Five minute load average.
    pub five: f32,
    /// Fifteen minute load average.
    pub fifteen: f32,
}

/// Physical and swap memory in bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, ToSchema)]
pub struct Memory {
    /// Total physical memory.
    pub total_bytes: u64,
    /// Memory in use, derived as total minus available.
    pub used_bytes: u64,
    /// Memory the kernel expects to be reclaimable for new work.
    pub available_bytes: u64,
    /// Page cache and reclaimable slab, minus shared memory.
    pub cache_bytes: u64,
    /// Total swap space.
    pub swap_total_bytes: u64,
    /// Swap space in use.
    pub swap_used_bytes: u64,
}

/// A mounted filesystem with its capacity and I/O rates.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, ToSchema)]
pub struct DiskMount {
    /// Device as the kernel reports it in `/proc/mounts`.
    pub device: String,
    /// Resolved `/proc/diskstats` name of the backing device, used to avoid
    /// counting one device twice when it is mounted under several paths.
    pub device_name: String,
    /// Directory the filesystem is mounted on.
    pub mount_point: String,
    /// Filesystem type, for example `ext4`.
    pub filesystem: String,
    /// Filesystem capacity.
    pub total_bytes: u64,
    /// Bytes in use by files.
    pub used_bytes: u64,
    /// Bytes available to unprivileged users.
    pub available_bytes: u64,
    /// Read throughput of the backing device in bytes per second.
    pub read_bytes_per_sec: u64,
    /// Write throughput of the backing device in bytes per second.
    pub write_bytes_per_sec: u64,
}

/// A network interface with its traffic rates.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, ToSchema)]
pub struct NetworkInterface {
    /// Interface name, for example `eth0`.
    pub name: String,
    /// Received bytes per second.
    pub rx_bytes_per_sec: u64,
    /// Transmitted bytes per second.
    pub tx_bytes_per_sec: u64,
}

/// A reading from a temperature sensor.
#[derive(Debug, Clone, Default, PartialEq, Serialize, ToSchema)]
pub struct Temperature {
    /// Sensor chip name, for example `coretemp`.
    pub chip: String,
    /// Sensor label within the chip.
    pub label: String,
    /// Temperature in degrees Celsius.
    pub celsius: f32,
}

/// A process and its resource usage at sample time.
#[derive(Debug, Clone, Default, PartialEq, Serialize, ToSchema)]
pub struct Process {
    /// Process identifier.
    pub pid: u32,
    /// Command name, truncated by the kernel to 15 characters.
    pub name: String,
    /// CPU usage in percent of one core; it can exceed 100 for multi-threaded
    /// processes.
    pub cpu_percent: f32,
    /// Resident memory in bytes.
    pub memory_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::{
        Cpu, DiskMount, LoadAverage, Memory, NetworkInterface, Process, Snapshot, Temperature,
    };

    #[test]
    fn snapshot_serialises_its_nested_types() {
        let snapshot = Snapshot {
            timestamp_ms: 1_700_000_000_000,
            uptime_secs: 12.5,
            cpu: Cpu {
                usage_percent: 25.0,
                per_core_percent: vec![20.0, 30.0],
                load: LoadAverage {
                    one: 0.5,
                    five: 0.4,
                    fifteen: 0.3,
                },
            },
            memory: Memory {
                total_bytes: 1000,
                used_bytes: 400,
                available_bytes: 600,
                cache_bytes: 100,
                swap_total_bytes: 200,
                swap_used_bytes: 50,
            },
            disks: vec![DiskMount {
                device: "/dev/sda2".to_owned(),
                device_name: "sda2".to_owned(),
                mount_point: "/".to_owned(),
                filesystem: "ext4".to_owned(),
                total_bytes: 1000,
                used_bytes: 400,
                available_bytes: 600,
                read_bytes_per_sec: 10,
                write_bytes_per_sec: 20,
            }],
            network: vec![NetworkInterface {
                name: "eth0".to_owned(),
                rx_bytes_per_sec: 1,
                tx_bytes_per_sec: 2,
            }],
            temperatures: vec![Temperature {
                chip: "coretemp".to_owned(),
                label: "Package id 0".to_owned(),
                celsius: 42.5,
            }],
            top_processes_by_cpu: vec![Process {
                pid: 7,
                name: "rustc".to_owned(),
                cpu_percent: 12.5,
                memory_bytes: 2048,
            }],
            top_processes_by_memory: Vec::new(),
        };

        let json = serde_json::to_value(&snapshot).expect("serialise snapshot");

        assert_eq!(json["cpu"]["per_core_percent"][1], 30.0);
        assert_eq!(json["memory"]["cache_bytes"], 100);
        assert_eq!(json["disks"][0]["mount_point"], "/");
        assert_eq!(json["network"][0]["name"], "eth0");
        assert_eq!(json["temperatures"][0]["celsius"], 42.5);
        assert_eq!(json["top_processes_by_cpu"][0]["pid"], 7);
        assert_eq!(json["top_processes_by_memory"], serde_json::json!([]));
    }
}
