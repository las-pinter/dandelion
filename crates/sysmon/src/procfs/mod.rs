//! Access to the kernel's procfs and sysfs counters.
//!
//! Every parser lives in its own module and works on a string, so it can be
//! tested against captured data without touching the live machine.

mod cpu;
mod disk;
mod memory;
mod network;
mod processes;
mod system;
mod temperature;

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::error::Error;
use crate::types::{LoadAverage, Temperature};

pub(crate) use cpu::{CpuCounters, CpuTimes};
pub(crate) use disk::{DiskCounters, DiskUsage, MountEntry};
pub(crate) use memory::MemInfo;
pub(crate) use network::NetCounters;
pub(crate) use processes::ProcessCounters;

/// Reads the counters the collector needs from `/proc` and `/sys`.
#[derive(Debug, Clone)]
pub(crate) struct Procfs {
    proc_root: PathBuf,
    hwmon_root: PathBuf,
}

impl Procfs {
    pub(crate) fn new() -> Self {
        Self {
            proc_root: PathBuf::from("/proc"),
            hwmon_root: PathBuf::from("/sys/class/hwmon"),
        }
    }

    pub(crate) fn cpu_times(&self) -> Result<CpuTimes, Error> {
        let text = self.read("stat")?;
        cpu::parse(&text).ok_or_else(|| Error::malformed(self.path("stat")))
    }

    pub(crate) fn memory(&self) -> Result<MemInfo, Error> {
        let text = self.read("meminfo")?;
        memory::parse(&text).ok_or_else(|| Error::malformed(self.path("meminfo")))
    }

    pub(crate) fn load_average(&self) -> Result<LoadAverage, Error> {
        let text = self.read("loadavg")?;
        system::parse_load(&text).ok_or_else(|| Error::malformed(self.path("loadavg")))
    }

    pub(crate) fn uptime_secs(&self) -> Result<f64, Error> {
        let text = self.read("uptime")?;
        system::parse_uptime(&text).ok_or_else(|| Error::malformed(self.path("uptime")))
    }

    pub(crate) fn network(&self) -> Result<BTreeMap<String, NetCounters>, Error> {
        let text = self.read("net/dev")?;
        network::parse(&text).ok_or_else(|| Error::malformed(self.path("net/dev")))
    }

    pub(crate) fn disk_counters(&self) -> Result<BTreeMap<String, DiskCounters>, Error> {
        let text = self.read("diskstats")?;
        disk::parse_diskstats(&text).ok_or_else(|| Error::malformed(self.path("diskstats")))
    }

    pub(crate) fn mounts(&self) -> Result<Vec<MountEntry>, Error> {
        let text = self.read("mounts")?;
        Ok(disk::parse_mounts(&text))
    }

    pub(crate) fn processes(&self) -> Result<Vec<ProcessCounters>, Error> {
        let directory = self.proc_root.as_path();
        let entries = std::fs::read_dir(directory)
            .map_err(|source| Error::io(directory.display().to_string(), source))?;

        let mut processes = Vec::new();
        for entry in entries.flatten() {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<u32>().ok())
            else {
                continue;
            };
            let stat_path = entry.path().join("stat");
            let Ok(text) = std::fs::read_to_string(&stat_path) else {
                // The process may exit between listing and reading it.
                continue;
            };
            if let Some(process) = processes::parse(&text)
                && process.pid == pid
            {
                processes.push(process);
            }
        }
        processes.sort_by_key(|process| process.pid);

        Ok(processes)
    }

    pub(crate) fn temperatures(&self) -> Vec<Temperature> {
        temperature::read_all(&self.hwmon_root)
    }

    fn read(&self, name: &str) -> Result<String, Error> {
        let path = self.proc_root.join(name);
        std::fs::read_to_string(&path)
            .map_err(|source| Error::io(path.display().to_string(), source))
    }

    fn path(&self, name: &str) -> String {
        self.proc_root.join(name).display().to_string()
    }
}

/// Reads capacity and usage of a mounted filesystem.
#[must_use]
pub(crate) fn disk_usage(mount_point: &str) -> Option<DiskUsage> {
    disk::usage(mount_point)
}

/// Maps a mount source to the `/proc/diskstats` device name.
#[must_use]
pub(crate) fn device_name(source: &str) -> String {
    disk::device_name(source)
}
