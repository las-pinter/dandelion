//! Process listing, signals and renicing.
//!
//! Listing reads `/proc` directly. Signals and nice values go through the
//! `kill(2)` and `setpriority(2)` system calls. Pid 0 (the kernel scheduler)
//! and pid 1 (init) are refused: the server may control this machine, but it
//! must not be able to stop its own init from a stray request.

mod list;
mod parse;

use rustix::io::Errno;
use rustix::process::{Pid, Signal, kill_process, setpriority_process};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// A process as shown in the process table.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProcessInfo {
    /// Process id.
    pub pid: u32,
    /// Parent process id, absent for kernel threads and init.
    pub parent_pid: Option<u32>,
    /// Executable name from `/proc/<pid>/stat`.
    pub name: String,
    /// Owning user name, or the numeric uid when it has no passwd entry.
    pub user: String,
    /// Scheduling state.
    pub state: ProcessState,
    /// Total CPU time consumed since start, in seconds.
    pub cpu_seconds: f64,
    /// Resident memory in bytes.
    pub memory_bytes: u64,
    /// Full command line, or the executable name when it has no arguments.
    pub command: String,
    /// Process start time as a Unix timestamp in seconds.
    pub started_at_unix: Option<i64>,
    /// Nice value between -20 and 19.
    pub niceness: i32,
}

/// One process and its children, for a tree view.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProcessNode {
    /// The process at this node.
    pub process: ProcessInfo,
    /// Child processes, in pid order.
    pub children: Vec<ProcessNode>,
}

/// Scheduling state of a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProcessState {
    /// Running or runnable.
    Running,
    /// Interruptible sleep.
    Sleeping,
    /// Uninterruptible disk sleep.
    DiskSleep,
    /// Stopped by a signal.
    Stopped,
    /// Stopped while being traced.
    TracingStop,
    /// Zombie: exited, not yet reaped.
    Zombie,
    /// Idle kernel thread.
    Idle,
    /// Dead.
    Dead,
    /// A state this crate does not map.
    Unknown,
}

impl ProcessState {
    /// Maps the one-letter state from `/proc/<pid>/stat`.
    #[must_use]
    pub const fn from_letter(letter: char) -> Self {
        match letter {
            'R' => Self::Running,
            'S' => Self::Sleeping,
            'D' => Self::DiskSleep,
            'T' => Self::Stopped,
            't' => Self::TracingStop,
            'Z' => Self::Zombie,
            'I' => Self::Idle,
            'X' | 'x' => Self::Dead,
            _ => Self::Unknown,
        }
    }
}

/// A signal this crate is allowed to send.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ProcessSignal {
    /// Graceful termination (`SIGTERM`).
    Term,
    /// Forced termination (`SIGKILL`).
    Kill,
    /// Interrupt (`SIGINT`).
    Int,
    /// Stop the process (`SIGSTOP`).
    Stop,
    /// Resume the process (`SIGCONT`).
    Cont,
}

impl ProcessSignal {
    /// The signal number.
    #[must_use]
    pub const fn number(self) -> i32 {
        self.as_signal().as_raw()
    }

    const fn as_signal(self) -> Signal {
        match self {
            Self::Term => Signal::TERM,
            Self::Kill => Signal::KILL,
            Self::Int => Signal::INT,
            Self::Stop => Signal::STOP,
            Self::Cont => Signal::CONT,
        }
    }

    /// The short name used in errors and logs.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Term => "TERM",
            Self::Kill => "KILL",
            Self::Int => "INT",
            Self::Stop => "STOP",
            Self::Cont => "CONT",
        }
    }
}

impl std::fmt::Display for ProcessSignal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// A request to signal a process.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SignalRequest {
    /// Target process id. Must be greater than 1.
    pub pid: i32,
    /// Signal to send.
    pub signal: ProcessSignal,
}

/// A request to change a process's nice value.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ReniceRequest {
    /// Target process id. Must be greater than 1.
    pub pid: i32,
    /// New nice value, between -20 and 19. Raising the priority (a smaller
    /// number) needs `CAP_SYS_NICE`.
    pub priority: i32,
}

/// Errors from the process domain.
#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    /// The target pid is 0 or 1, or is not a single process.
    #[error("refusing to target pid {pid}: only pids above 1 may be signalled or reniced")]
    UnsafeTarget {
        /// The rejected pid.
        pid: i32,
    },
    /// The nice value is outside the kernel's range.
    #[error("invalid nice value {priority}: must be between -20 and 19")]
    InvalidNice {
        /// The rejected nice value.
        priority: i32,
    },
    /// The target process no longer exists.
    #[error("process {pid} does not exist")]
    NoSuchProcess {
        /// The missing pid.
        pid: i32,
    },
    /// The process exists but this user may not touch it.
    #[error("permission denied for process {pid}")]
    PermissionDenied {
        /// The pid that was refused.
        pid: i32,
    },
    /// A `/proc` file could not be read.
    #[error("failed to read {}: {source}", path.display())]
    Io {
        /// The path that failed.
        path: std::path::PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
    /// A `/proc` file did not have the expected shape.
    #[error("malformed {source_file}: {reason}")]
    Malformed {
        /// The file kind, such as `stat`.
        source_file: &'static str,
        /// What was wrong with it.
        reason: &'static str,
    },
    /// Sending the signal failed.
    #[error("signal {signal} to process {pid} failed: {source}")]
    SignalFailed {
        /// The target pid.
        pid: i32,
        /// The signal that was attempted.
        signal: ProcessSignal,
        /// The kernel error.
        #[source]
        source: std::io::Error,
    },
    /// Changing the nice value failed.
    #[error("renice process {pid} to {priority} failed: {source}")]
    ReniceFailed {
        /// The target pid.
        pid: i32,
        /// The requested nice value.
        priority: i32,
        /// The kernel error.
        #[source]
        source: std::io::Error,
    },
}

/// Reads processes and mutates their signals and nice values.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessService;

impl ProcessService {
    /// Creates a process service.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Lists every readable process, sorted by pid.
    ///
    /// Processes that exit mid-scan, or that the kernel hides from this user,
    /// are omitted rather than reported as errors.
    pub fn list(&self) -> Result<Vec<ProcessInfo>, ProcessError> {
        list::list_processes()
    }

    /// Lists every readable process as a forest, sorted by pid at each level.
    pub fn tree(&self) -> Result<Vec<ProcessNode>, ProcessError> {
        list::list_processes().map(|processes| list::build_tree(&processes))
    }

    /// Sends one of the supported signals to a process.
    pub fn signal(&self, request: &SignalRequest) -> Result<(), ProcessError> {
        let pid = checked_pid(request.pid)?;
        kill_process(pid, request.signal.as_signal()).map_err(|errno| match errno {
            Errno::SRCH => ProcessError::NoSuchProcess { pid: request.pid },
            Errno::PERM => ProcessError::PermissionDenied { pid: request.pid },
            other => ProcessError::SignalFailed {
                pid: request.pid,
                signal: request.signal,
                source: other.into(),
            },
        })
    }

    /// Changes a process's nice value.
    pub fn renice(&self, request: &ReniceRequest) -> Result<(), ProcessError> {
        let pid = checked_pid(request.pid)?;
        if !(-20..=19).contains(&request.priority) {
            return Err(ProcessError::InvalidNice {
                priority: request.priority,
            });
        }
        setpriority_process(Some(pid), request.priority).map_err(|errno| match errno {
            Errno::SRCH => ProcessError::NoSuchProcess { pid: request.pid },
            Errno::PERM => ProcessError::PermissionDenied { pid: request.pid },
            other => ProcessError::ReniceFailed {
                pid: request.pid,
                priority: request.priority,
                source: other.into(),
            },
        })
    }
}

/// Refuses pids that denote more than one process, or the kernel scheduler and
/// init, which the server must never signal by accident.
fn checked_pid(pid: i32) -> Result<Pid, ProcessError> {
    if pid <= 1 {
        return Err(ProcessError::UnsafeTarget { pid });
    }
    Pid::from_raw(pid).ok_or(ProcessError::UnsafeTarget { pid })
}

#[cfg(test)]
mod tests {
    use super::{
        ProcessError, ProcessService, ProcessSignal, ProcessState, ReniceRequest, SignalRequest,
    };

    #[test]
    fn state_letters_map_to_states() {
        assert_eq!(ProcessState::from_letter('R'), ProcessState::Running);
        assert_eq!(ProcessState::from_letter('S'), ProcessState::Sleeping);
        assert_eq!(ProcessState::from_letter('D'), ProcessState::DiskSleep);
        assert_eq!(ProcessState::from_letter('T'), ProcessState::Stopped);
        assert_eq!(ProcessState::from_letter('t'), ProcessState::TracingStop);
        assert_eq!(ProcessState::from_letter('Z'), ProcessState::Zombie);
        assert_eq!(ProcessState::from_letter('I'), ProcessState::Idle);
        assert_eq!(ProcessState::from_letter('X'), ProcessState::Dead);
        assert_eq!(ProcessState::from_letter('?'), ProcessState::Unknown);
    }

    #[test]
    fn signal_numbers_match_the_kernel_constants() {
        use rustix::process::Signal;

        assert_eq!(ProcessSignal::Term.number(), Signal::TERM.as_raw());
        assert_eq!(ProcessSignal::Kill.number(), Signal::KILL.as_raw());
        assert_eq!(ProcessSignal::Int.number(), Signal::INT.as_raw());
        assert_eq!(ProcessSignal::Stop.number(), Signal::STOP.as_raw());
        assert_eq!(ProcessSignal::Cont.number(), Signal::CONT.as_raw());
        assert_eq!(ProcessSignal::Kill.to_string(), "KILL");
    }

    #[test]
    fn refusing_pid_zero_and_init_happens_before_the_syscall() {
        let service = ProcessService::new();
        for pid in [0, 1, -5] {
            let signal = SignalRequest {
                pid,
                signal: ProcessSignal::Term,
            };
            let err = service
                .signal(&signal)
                .expect_err("pid at or below 1 must be refused");
            assert!(matches!(err, ProcessError::UnsafeTarget { pid: got } if got == pid));

            let renice = ReniceRequest { pid, priority: 5 };
            let err = service
                .renice(&renice)
                .expect_err("pid at or below 1 must be refused");
            assert!(matches!(err, ProcessError::UnsafeTarget { pid: got } if got == pid));
        }
    }

    #[test]
    fn nice_value_range_is_checked_before_the_syscall() {
        let service = ProcessService::new();
        for priority in [-21, 20] {
            let err = service
                .renice(&ReniceRequest {
                    pid: i32::try_from(std::process::id()).expect("test pid fits i32"),
                    priority,
                })
                .expect_err("nice value out of range must be refused");
            assert!(matches!(err, ProcessError::InvalidNice { priority: got } if got == priority));
        }
    }

    #[test]
    fn signalling_an_unknown_pid_reports_no_such_process() {
        let service = ProcessService::new();
        // Pid i32::MAX is above the kernel's pid ceiling and cannot be live.
        let err = service
            .signal(&SignalRequest {
                pid: i32::MAX,
                signal: ProcessSignal::Term,
            })
            .expect_err("unknown pid must fail");
        assert!(matches!(err, ProcessError::NoSuchProcess { pid } if pid == i32::MAX));
    }

    #[test]
    fn signal_request_serializes_signal_as_lowercase_name() {
        let json = serde_json::to_string(&SignalRequest {
            pid: 42,
            signal: ProcessSignal::Kill,
        })
        .expect("request must serialize");
        assert_eq!(json, r#"{"pid":42,"signal":"kill"}"#);
    }
}
