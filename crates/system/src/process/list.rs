//! Builds process rows from `/proc` and arranges them into a forest.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::parse::{self, ProcStat};
use super::{ProcessError, ProcessInfo, ProcessNode};

const PROC_ROOT: &str = "/proc";
const PASSWD: &str = "/etc/passwd";
const PROC_STAT: &str = "/proc/stat";

/// Lists every process this user can read. A process that exits mid-scan, or
/// whose files are hidden by the kernel, is omitted instead of failing the
/// whole listing.
pub(crate) fn list_processes() -> Result<Vec<ProcessInfo>, ProcessError> {
    let users =
        fs::read_to_string(PASSWD).map_or_else(|_| HashMap::new(), |raw| parse::parse_passwd(&raw));
    let boot_time = fs::read_to_string(PROC_STAT)
        .ok()
        .and_then(|raw| parse::parse_btime(&raw));
    let ticks = parse::clock_ticks_per_second();
    let page_bytes = parse::page_size();

    let entries = fs::read_dir(PROC_ROOT).map_err(|source| ProcessError::Io {
        path: PathBuf::from(PROC_ROOT),
        source,
    })?;
    let mut processes = Vec::new();
    for entry in entries {
        let Ok(entry) = entry else { continue };
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        if let Ok(info) = read_process(pid, &users, boot_time, ticks, page_bytes) {
            processes.push(info);
        }
    }
    processes.sort_by_key(|process| process.pid);
    Ok(processes)
}

fn read_process(
    pid: u32,
    users: &HashMap<u32, String>,
    boot_time: Option<i64>,
    ticks: u64,
    page_bytes: u64,
) -> Result<ProcessInfo, ProcessError> {
    let directory = Path::new(PROC_ROOT).join(pid.to_string());
    let stat = parse::parse_stat(&read_to_string(&directory.join("stat"))?)?;
    let uid = parse::parse_status_uid(&read_to_string(&directory.join("status"))?).ok_or(
        ProcessError::Malformed {
            source_file: "status",
            reason: "missing Uid line",
        },
    )?;
    let command = fs::read(directory.join("cmdline"))
        .ok()
        .and_then(|raw| parse::parse_cmdline(&raw))
        .unwrap_or_else(|| stat.comm.clone());
    Ok(ProcessInfo {
        pid,
        parent_pid: (stat.ppid != 0).then_some(stat.ppid),
        name: stat.comm.clone(),
        user: users.get(&uid).cloned().unwrap_or_else(|| uid.to_string()),
        state: stat.state,
        cpu_seconds: cpu_seconds(&stat, ticks),
        memory_bytes: memory_bytes(&stat, page_bytes),
        command,
        started_at_unix: boot_time.map(|boot| boot + start_offset_secs(&stat, ticks)),
        niceness: stat.nice,
    })
}

fn read_to_string(path: &Path) -> Result<String, ProcessError> {
    fs::read_to_string(path).map_err(|source| ProcessError::Io {
        path: path.to_path_buf(),
        source,
    })
}

#[allow(clippy::cast_precision_loss)]
fn cpu_seconds(stat: &ProcStat, ticks: u64) -> f64 {
    let total = stat.utime_ticks.saturating_add(stat.stime_ticks);
    total as f64 / ticks as f64
}

fn memory_bytes(stat: &ProcStat, page_bytes: u64) -> u64 {
    u64::try_from(stat.rss_pages.max(0))
        .unwrap_or(0)
        .saturating_mul(page_bytes)
}

fn start_offset_secs(stat: &ProcStat, ticks: u64) -> i64 {
    i64::try_from(stat.start_ticks / ticks).unwrap_or(i64::MAX)
}

/// Arranges processes into a forest. A process whose parent is missing from the
/// list becomes a root; a parent cycle (which the kernel never produces) is
/// broken by making the first unvisited member a root instead of recursing
/// forever.
pub(crate) fn build_tree(processes: &[ProcessInfo]) -> Vec<ProcessNode> {
    let positions: HashMap<u32, usize> = processes
        .iter()
        .enumerate()
        .map(|(index, process)| (process.pid, index))
        .collect();
    let mut children: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut roots = Vec::new();
    for (index, process) in processes.iter().enumerate() {
        match process
            .parent_pid
            .and_then(|parent| positions.get(&parent).copied())
        {
            Some(parent_index) => children.entry(parent_index).or_default().push(index),
            None => roots.push(index),
        }
    }

    let mut visited = vec![false; processes.len()];
    let mut forest = Vec::new();
    for index in roots {
        if let Some(process) = processes.get(index) {
            forest.push(build_node(
                process,
                index,
                processes,
                &children,
                &mut visited,
            ));
        }
    }
    for index in 0..processes.len() {
        if visited.get(index).copied().unwrap_or(true) {
            continue;
        }
        if let Some(process) = processes.get(index) {
            forest.push(build_node(
                process,
                index,
                processes,
                &children,
                &mut visited,
            ));
        }
    }
    forest
}

fn build_node(
    process: &ProcessInfo,
    index: usize,
    processes: &[ProcessInfo],
    children: &HashMap<usize, Vec<usize>>,
    visited: &mut Vec<bool>,
) -> ProcessNode {
    if let Some(seen) = visited.get_mut(index) {
        *seen = true;
    }
    let mut nodes = Vec::new();
    if let Some(child_indices) = children.get(&index) {
        for child in child_indices {
            if visited.get(*child).copied().unwrap_or(true) {
                continue;
            }
            if let Some(child_process) = processes.get(*child) {
                nodes.push(build_node(
                    child_process,
                    *child,
                    processes,
                    children,
                    visited,
                ));
            }
        }
    }
    ProcessNode {
        process: process.clone(),
        children: nodes,
    }
}

#[cfg(test)]
mod tests {
    use super::{build_tree, list_processes, read_process};
    use crate::process::{ProcessInfo, ProcessState};
    use std::collections::HashMap;

    fn info(pid: u32, parent_pid: Option<u32>) -> ProcessInfo {
        ProcessInfo {
            pid,
            parent_pid,
            name: format!("proc-{pid}"),
            user: "probe".to_owned(),
            state: ProcessState::Sleeping,
            cpu_seconds: 0.0,
            memory_bytes: 0,
            command: format!("proc-{pid}"),
            started_at_unix: Some(1_700_000_000),
            niceness: 0,
        }
    }

    #[test]
    fn tree_keeps_the_hierarchy_and_orders_by_pid() {
        let processes = &[
            info(1, None),
            info(30, Some(10)),
            info(10, Some(1)),
            info(20, Some(1)),
            info(40, Some(30)),
        ];

        let forest = build_tree(processes);

        assert_eq!(forest.len(), 1);
        let init = &forest[0];
        assert_eq!(init.process.pid, 1);
        assert_eq!(
            init.children
                .iter()
                .map(|node| node.process.pid)
                .collect::<Vec<_>>(),
            vec![10, 20]
        );
        let ten = &init.children[0];
        assert_eq!(ten.children.len(), 1);
        assert_eq!(ten.children[0].process.pid, 30);
        assert_eq!(ten.children[0].children[0].process.pid, 40);
    }

    #[test]
    fn missing_parent_makes_a_root() {
        let forest = build_tree(&[info(100, Some(999)), info(101, None)]);

        assert_eq!(forest.len(), 2);
        assert_eq!(forest[0].process.pid, 100);
        assert_eq!(forest[1].process.pid, 101);
    }

    #[test]
    fn a_parent_cycle_does_not_recurse_forever() {
        let forest = build_tree(&[info(2, Some(3)), info(3, Some(2))]);

        assert_eq!(forest.len(), 1);
        assert_eq!(forest[0].process.pid, 2);
        assert_eq!(forest[0].children[0].process.pid, 3);
        assert!(forest[0].children[0].children.is_empty());
    }

    #[test]
    fn reading_this_test_process_fills_every_field() {
        let users = HashMap::new();
        let ticks = super::parse::clock_ticks_per_second();
        let pages = super::parse::page_size();
        let boot_time = std::fs::read_to_string(super::PROC_STAT)
            .ok()
            .and_then(|raw| super::parse::parse_btime(&raw));

        let pid = std::process::id();
        let process = read_process(pid, &users, boot_time, ticks, pages)
            .expect("this test process must be readable");

        assert_eq!(process.pid, pid);
        assert!(process.parent_pid.is_some());
        assert_ne!(process.name, "");
        assert_ne!(process.user, "");
        assert_ne!(process.command, "");
        assert!(process.started_at_unix.is_some_and(|start| start > 0));
        assert!(process.memory_bytes > 0);
    }

    #[test]
    fn listing_includes_this_test_process_in_pid_order() {
        let processes = list_processes().expect("/proc must be readable");

        assert!(processes.windows(2).all(|pair| pair[0].pid <= pair[1].pid));
        let pid = std::process::id();
        assert!(processes.iter().any(|process| process.pid == pid));
    }
}
