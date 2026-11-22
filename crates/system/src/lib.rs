//! Machine control: processes, systemd units and Docker containers.
//!
//! Every mutating call takes a typed request, so the server can audit the
//! request before it runs. Read-only calls return serializable types that the
//! HTTP layer can serve directly.
//!
//! The crate is split into three domains:
//!
//! - [`process`] reads `/proc` and sends signals or reniced pids.
//! - [`systemd`] talks to the user and system managers over D-Bus, reads the
//!   journal, and routes system-unit actions through a `sudo -n` allowlist.
//! - [`docker`] uses the Docker Engine API on the local socket and reports an
//!   unreachable socket as unavailable rather than as a failure.
//!
//! Tests that touch real processes, units, or containers start their own
//! throwaway subject and are marked `#[ignore]`.

mod error;

pub mod docker;
pub mod process;
pub mod systemd;

pub use error::SystemError;
