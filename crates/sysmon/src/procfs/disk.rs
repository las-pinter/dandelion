//! Parsers for `/proc/diskstats` and `/proc/mounts`, plus filesystem usage.

use std::collections::BTreeMap;
use std::path::Path;

/// Cumulative block-device counters, in bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct DiskCounters {
    pub(crate) read_bytes: u64,
    pub(crate) write_bytes: u64,
}

/// Capacity of a mounted filesystem, in bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct DiskUsage {
    pub(crate) total: u64,
    pub(crate) used: u64,
    pub(crate) available: u64,
}

/// One real filesystem mount point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MountEntry {
    pub(crate) device: String,
    pub(crate) mount_point: String,
    pub(crate) fs_type: String,
}

/// Filesystem types that are kernel interfaces rather than storage.
const VIRTUAL_FS: &[&str] = &[
    "autofs",
    "bpf",
    "cgroup",
    "cgroup2",
    "configfs",
    "debugfs",
    "devpts",
    "devtmpfs",
    "efivarfs",
    "fusectl",
    "hugetlbfs",
    "mqueue",
    "nsfs",
    "proc",
    "pstore",
    "ramfs",
    "rpc_pipefs",
    "securityfs",
    "squashfs",
    "sysfs",
    "tmpfs",
    "tracefs",
];

/// Sectors in `/proc/diskstats` are always 512 bytes, independent of the
/// device's logical block size.
const SECTOR_BYTES: u64 = 512;

/// Parses block-device counters from `/proc/diskstats`.
///
/// Returns `None` when no line is valid.
pub(crate) fn parse_diskstats(text: &str) -> Option<BTreeMap<String, DiskCounters>> {
    let mut devices = BTreeMap::new();

    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 10 {
            continue;
        }
        let name = fields[2];
        let (Ok(sectors_read), Ok(sectors_written)) =
            (fields[5].parse::<u64>(), fields[9].parse::<u64>())
        else {
            continue;
        };

        devices.insert(
            name.to_owned(),
            DiskCounters {
                read_bytes: sectors_read.saturating_mul(SECTOR_BYTES),
                write_bytes: sectors_written.saturating_mul(SECTOR_BYTES),
            },
        );
    }

    if devices.is_empty() {
        None
    } else {
        Some(devices)
    }
}

/// Parses real filesystem mounts from `/proc/mounts`.
///
/// Kernel interfaces and image mounts are skipped. Paths are unescaped from
/// the octal form the kernel uses for spaces and tabs.
pub(crate) fn parse_mounts(text: &str) -> Vec<MountEntry> {
    let mut mounts = Vec::new();

    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 3 {
            continue;
        }
        let fs_type = fields[2];
        if VIRTUAL_FS.contains(&fs_type) {
            continue;
        }

        mounts.push(MountEntry {
            device: decode_path(fields[0]),
            mount_point: decode_path(fields[1]),
            fs_type: fs_type.to_owned(),
        });
    }

    mounts
}

/// Maps a mount source to the device name `/proc/diskstats` uses.
///
/// Canonicalisation resolves `/dev/mapper` and `/dev/disk/by-*` symlinks.
pub(crate) fn device_name(source: &str) -> String {
    let resolved =
        std::fs::canonicalize(source).unwrap_or_else(|_| Path::new(source).to_path_buf());

    resolved.file_name().map_or_else(
        || source.to_owned(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// Reads capacity and usage of a mount point.
#[must_use]
pub(crate) fn usage(mount_point: &str) -> Option<DiskUsage> {
    let stats = rustix::fs::statvfs(mount_point).ok()?;
    let block_size = stats.f_frsize;
    let total = stats.f_blocks.saturating_mul(block_size);
    let free = stats.f_bfree.saturating_mul(block_size);
    let available = stats.f_bavail.saturating_mul(block_size);

    Some(DiskUsage {
        total,
        used: total.saturating_sub(free),
        available,
    })
}

fn decode_path(path: &str) -> String {
    let mut decoded = String::with_capacity(path.len());
    let mut chars = path.chars();

    while let Some(character) = chars.next() {
        if character != '\\' {
            decoded.push(character);
            continue;
        }

        let mut code = String::with_capacity(3);
        for _ in 0..3 {
            let Some(digit) = chars.next() else {
                break;
            };
            code.push(digit);
        }

        match code.as_str() {
            "040" => decoded.push(' '),
            "011" => decoded.push('\t'),
            "012" => decoded.push('\n'),
            "134" => decoded.push('\\'),
            _ => {
                decoded.push('\\');
                decoded.push_str(&code);
            }
        }
    }

    decoded
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{DiskCounters, parse_diskstats, parse_mounts, usage};

    const DISKSTATS: &str = "\
   7       0 loop0 14 0 34 5 0 0 0 0 0 6 5 0 0 0 0 0 0
   8       0 sda 100 0 2000 10 50 0 4000 20 0 30 40 0 0 0 0 0 0
   8       2 sda2 90 0 1800 9 40 0 3600 18 0 30 40 0 0 0 0 0 0
";

    #[test]
    fn parses_sector_counters_as_bytes() {
        let parsed = parse_diskstats(DISKSTATS).expect("parse diskstats");

        assert_eq!(
            parsed,
            BTreeMap::from([
                (
                    "loop0".to_owned(),
                    DiskCounters {
                        read_bytes: 34 * 512,
                        write_bytes: 0,
                    },
                ),
                (
                    "sda".to_owned(),
                    DiskCounters {
                        read_bytes: 2000 * 512,
                        write_bytes: 4000 * 512,
                    },
                ),
                (
                    "sda2".to_owned(),
                    DiskCounters {
                        read_bytes: 1800 * 512,
                        write_bytes: 3600 * 512,
                    },
                ),
            ])
        );
    }

    #[test]
    fn skips_short_and_non_numeric_rows() {
        assert!(parse_diskstats("8 0 sda 1 2\n8 1 sdb x 0 y 2 3 4 z 6\n").is_none());
    }

    const MOUNTS: &str = "\
sysfs /sys sysfs rw,nosuid 0 0
proc /proc proc rw,nosuid 0 0
tmpfs /run tmpfs rw,nosuid 0 0
/dev/sda2 / ext4 rw,relatime 0 0
/dev/sda2 /mnt/my\\040disk ext4 rw,relatime 0 0
/dev/loop3 /snap/core24/1643 squashfs ro,nodev 0 0
overlay /srv/overlay overlay rw,lowerdir=/a 0 0
";

    #[test]
    fn keeps_real_filesystems_and_unescapes_paths() {
        let mounts = parse_mounts(MOUNTS);

        let shown: Vec<(&str, &str, &str)> = mounts
            .iter()
            .map(|mount| {
                (
                    mount.device.as_str(),
                    mount.mount_point.as_str(),
                    mount.fs_type.as_str(),
                )
            })
            .collect();

        assert_eq!(
            shown,
            vec![
                ("/dev/sda2", "/", "ext4"),
                ("/dev/sda2", "/mnt/my disk", "ext4"),
                ("overlay", "/srv/overlay", "overlay"),
            ]
        );
    }

    #[test]
    fn reports_usage_of_the_root_filesystem() {
        let usage = usage("/").expect("statvfs /");

        assert!(usage.total > 0);
        assert!(usage.used <= usage.total);
        assert!(usage.available <= usage.total);
    }

    #[test]
    fn missing_mount_point_has_no_usage() {
        assert!(usage("/definitely/not/a/mount/point").is_none());
    }
}
