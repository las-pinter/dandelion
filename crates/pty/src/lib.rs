//! Local pseudo-terminals for the web terminal.
//!
//! One [`Pty`] owns one shell on one pseudo-terminal. Reads and writes are
//! blocking, so the server drives them from its own threads; [`Pty::kill`]
//! ends the shell, which makes a blocked read return. The crate keeps the
//! workspace's `unsafe` ban by building on `portable-pty`, which owns the
//! `setsid` and controlling-terminal dance.

use std::{
    fmt,
    io::{Read, Write},
    path::PathBuf,
    sync::Mutex,
};

use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use rustix::process::{Pid, Signal, kill_process};
use thiserror::Error;

/// Largest terminal side this crate opens, in cells; larger requests clamp.
pub const MAX_DIMENSION: u16 = 500;
/// The `TERM` every shell is started with, which xterm.js understands.
pub const TERM: &str = "xterm-256color";

/// Why a pseudo-terminal could not be opened or driven.
#[derive(Debug, Error)]
pub enum PtyError {
    /// The pseudo-terminal pair could not be created.
    #[error("cannot open a pseudo-terminal: {0}")]
    Open(String),
    /// The shell process could not be started.
    #[error("cannot spawn the shell `{shell}`: {reason}")]
    Spawn {
        /// The shell that was attempted.
        shell: String,
        /// Why the spawn failed.
        reason: String,
    },
    /// The terminal was already closed or its locks were poisoned.
    #[error("the shell is gone")]
    Gone,
    /// The terminal size could not be applied.
    #[error("cannot resize the terminal: {0}")]
    Resize(String),
    /// Reading or writing the terminal failed.
    #[error("terminal I/O failed: {0}")]
    Io(#[from] std::io::Error),
}

/// How to open one shell.
#[derive(Debug, Clone)]
pub struct PtyOptions {
    /// Shell to run; defaults to `$SHELL`, then `/bin/sh`.
    pub shell: Option<String>,
    /// Initial width in cells.
    pub cols: u16,
    /// Initial height in cells.
    pub rows: u16,
    /// Working directory; defaults to the server's.
    pub cwd: Option<PathBuf>,
}

impl Default for PtyOptions {
    fn default() -> Self {
        Self {
            shell: None,
            cols: 80,
            rows: 24,
            cwd: None,
        }
    }
}

/// One shell on one pseudo-terminal.
pub struct Pty {
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    reader: Mutex<Box<dyn Read + Send>>,
    child: Mutex<Box<dyn Child + Send + Sync>>,
    shell: String,
    size: Mutex<(u16, u16)>,
}

impl fmt::Debug for Pty {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Pty")
            .field("shell", &self.shell)
            .field("size", &self.size())
            .finish_non_exhaustive()
    }
}

impl Pty {
    /// Opens a shell on a new pseudo-terminal.
    ///
    /// # Errors
    ///
    /// Returns [`PtyError::Open`] when the terminal pair cannot be created and
    /// [`PtyError::Spawn`] when the shell cannot be started.
    pub fn spawn(options: &PtyOptions) -> Result<Self, PtyError> {
        let size = PtySize {
            rows: clamp(options.rows),
            cols: clamp(options.cols),
            pixel_width: 0,
            pixel_height: 0,
        };
        let pair = native_pty_system()
            .openpty(size)
            .map_err(|error| PtyError::Open(error.to_string()))?;

        let shell = options
            .shell
            .clone()
            .or_else(|| std::env::var("SHELL").ok())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "/bin/sh".to_owned());
        let mut command = CommandBuilder::new(&shell);
        command.env("TERM", TERM);
        if let Some(cwd) = &options.cwd {
            command.cwd(cwd);
        }
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| PtyError::Spawn {
                shell: shell.clone(),
                reason: error.to_string(),
            })?;
        // Dropping the slave side is what lets a blocked read see EOF once the
        // shell exits.
        drop(pair.slave);

        let writer = pair
            .master
            .take_writer()
            .map_err(|error| PtyError::Open(error.to_string()))?;
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| PtyError::Open(error.to_string()))?;

        Ok(Self {
            master: Mutex::new(pair.master),
            writer: Mutex::new(writer),
            reader: Mutex::new(reader),
            child: Mutex::new(child),
            shell,
            size: Mutex::new((size.cols, size.rows)),
        })
    }

    /// The shell this terminal runs.
    #[must_use]
    pub fn shell(&self) -> &str {
        &self.shell
    }

    /// The current size in cells, as `(cols, rows)`.
    #[must_use]
    pub fn size(&self) -> (u16, u16) {
        self.size.lock().map_or((80, 24), |size| *size)
    }

    /// Reads up to `buf.len()` bytes of shell output, blocking until some
    /// arrive or the shell ends.
    ///
    /// # Errors
    ///
    /// Returns [`PtyError::Gone`] after the terminal closed and
    /// [`PtyError::Io`] for a failed read. Zero bytes means end of output.
    pub fn read(&self, buf: &mut [u8]) -> Result<usize, PtyError> {
        let mut reader = self.reader.lock().map_err(|_| PtyError::Gone)?;
        Ok(reader.read(buf)?)
    }

    /// Writes keystrokes to the shell.
    ///
    /// # Errors
    ///
    /// Returns [`PtyError::Gone`] after the terminal closed and
    /// [`PtyError::Io`] for a failed write.
    pub fn write(&self, data: &[u8]) -> Result<(), PtyError> {
        let mut writer = self.writer.lock().map_err(|_| PtyError::Gone)?;
        writer.write_all(data)?;
        writer.flush()?;
        Ok(())
    }

    /// Applies a new size, in cells.
    ///
    /// # Errors
    ///
    /// Returns [`PtyError::Resize`] when the terminal rejects the size. Both
    /// sides clamp to `1..=MAX_DIMENSION`.
    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), PtyError> {
        let cols = clamp(cols);
        let rows = clamp(rows);
        let master = self.master.lock().map_err(|_| PtyError::Gone)?;
        master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| PtyError::Resize(error.to_string()))?;
        drop(master);
        if let Ok(mut size) = self.size.lock() {
            *size = (cols, rows);
        }
        Ok(())
    }

    /// True while the shell process is still running.
    #[must_use]
    pub fn is_running(&self) -> bool {
        let mut child = match self.child.lock() {
            Ok(child) => child,
            Err(poisoned) => poisoned.into_inner(),
        };
        matches!(child.try_wait(), Ok(None))
    }

    /// Ends the shell and every process in its session.
    ///
    /// The polite SIGHUP alone is not enough: a background child holding the
    /// pseudo-terminal keeps the master readable, so a read would never see
    /// end-of-file and the caller's cleanup would hang. Every process whose
    /// session id is the shell's is killed, which closes all slave handles.
    pub fn kill(&self) {
        let pid = {
            let mut child = match self.child.lock() {
                Ok(child) => child,
                Err(poisoned) => poisoned.into_inner(),
            };
            let pid = child.process_id();
            if let Err(error) = child.kill() {
                tracing::debug!(%error, "cannot hang up the shell");
            }
            pid
        };
        let Some(leader) = pid else { return };
        for member in session_members(leader) {
            let Some(pid) = i32::try_from(member).ok().and_then(Pid::from_raw) else {
                continue;
            };
            if let Err(error) = kill_process(pid, Signal::KILL) {
                tracing::debug!(%error, pid = member, "cannot kill a terminal process");
            }
        }
    }
}

/// Every process whose session id is `leader`, from `/proc`.
fn session_members(leader: u32) -> Vec<u32> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    let mut members = Vec::new();
    for entry in entries.flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
            continue;
        };
        // The command name can contain spaces and parentheses, so split after
        // the last `)`; the fields that follow are state, ppid, pgrp, session.
        let Some((_, rest)) = stat.rsplit_once(')') else {
            continue;
        };
        let session = rest
            .split_whitespace()
            .nth(3)
            .and_then(|raw| raw.parse::<u32>().ok());
        if session == Some(leader) {
            members.push(pid);
        }
    }
    members
}

impl Drop for Pty {
    fn drop(&mut self) {
        self.kill();
    }
}

fn clamp(value: u16) -> u16 {
    value.clamp(1, MAX_DIMENSION)
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc,
            mpsc::{self, RecvTimeoutError},
        },
        thread,
        time::Duration,
    };

    use super::{MAX_DIMENSION, Pty, PtyError, PtyOptions};

    const TIMEOUT: Duration = Duration::from_secs(10);

    fn shell(options: &PtyOptions) -> Arc<Pty> {
        Arc::new(Pty::spawn(options).expect("the test shell opens"))
    }

    /// Reads until `needle` appears or the shell ends.
    fn expect_output(pty: &Arc<Pty>, needle: &'static str) -> String {
        let reader = Arc::clone(pty);
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let mut collected = String::new();
            let mut buf = [0u8; 1024];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(count) => {
                        collected.push_str(&String::from_utf8_lossy(&buf[..count]));
                        if collected.contains(needle) {
                            break;
                        }
                    }
                }
            }
            let _ = sender.send(collected);
        });
        match receiver.recv_timeout(TIMEOUT) {
            Ok(collected) => collected,
            Err(RecvTimeoutError::Timeout) => panic!("timed out waiting for {needle}"),
            Err(RecvTimeoutError::Disconnected) => panic!("the reader ended before {needle}"),
        }
    }

    #[test]
    fn a_shell_echoes_what_it_is_given() {
        let pty = shell(&PtyOptions {
            shell: Some("/bin/sh".to_owned()),
            ..PtyOptions::default()
        });

        pty.write(b"printf probe-ok\\n\n").expect("write");

        assert!(expect_output(&pty, "probe-ok").contains("probe-ok"));
        assert_eq!(pty.shell(), "/bin/sh");
    }

    #[test]
    fn sizes_are_clamped() {
        let pty = shell(&PtyOptions {
            shell: Some("/bin/sh".to_owned()),
            cols: 5000,
            rows: 0,
            cwd: None,
        });

        assert_eq!(pty.size(), (MAX_DIMENSION, 1));
        pty.resize(120, 40).expect("resize");
        assert_eq!(pty.size(), (120, 40));
        pty.resize(0, 5000)
            .expect("resize clamps rather than fails");
        assert_eq!(pty.size(), (1, MAX_DIMENSION));
    }

    #[test]
    fn kill_ends_the_shell() {
        let pty = shell(&PtyOptions {
            shell: Some("/bin/sh".to_owned()),
            ..PtyOptions::default()
        });
        assert!(pty.is_running());

        pty.kill();

        let deadline = std::time::Instant::now() + TIMEOUT;
        while pty.is_running() && std::time::Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        assert!(!pty.is_running(), "the shell ends after kill");
    }

    #[test]
    fn a_background_child_does_not_hold_the_terminal() {
        let pty = shell(&PtyOptions {
            shell: Some("/bin/sh".to_owned()),
            ..PtyOptions::default()
        });
        pty.write(b"sleep 1000 &\n").expect("write");
        // Let the shell start the job before the hang-up.
        thread::sleep(Duration::from_millis(300));

        pty.kill();

        let reader = Arc::clone(&pty);
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let mut buf = [0u8; 256];
            // Output already buffered in the master may arrive first; what
            // matters is that the read ends.
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => {
                        let _ = sender.send(());
                        return;
                    }
                    Ok(_) => {}
                }
            }
        });
        match receiver.recv_timeout(TIMEOUT) {
            Ok(()) => {}
            Err(error) => panic!("the read never ended; a child holds the terminal: {error}"),
        }
    }

    #[test]
    fn a_missing_shell_names_itself() {
        let error = Pty::spawn(&PtyOptions {
            shell: Some("/nonexistent/dandelion-shell".to_owned()),
            ..PtyOptions::default()
        })
        .expect_err("the shell cannot start");

        match error {
            PtyError::Spawn { shell, .. } => {
                assert_eq!(shell, "/nonexistent/dandelion-shell");
            }
            other => panic!("expected a spawn error, got {other:?}"),
        }
    }
}
