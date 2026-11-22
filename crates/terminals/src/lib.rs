//! Terminal and agent backends for Dandelion.
//!
//! Two implementations sit behind [`TerminalBackend`]:
//!
//! - [`herdr_core::HerdrCoreBackend`] talks JSON-RPC 2.0 to a herdr-core daemon
//!   over its unix socket and uses framed binary attach streams with CREDIT
//!   flow control.
//! - [`herdr::ClassicHerdrBackend`] talks line-delimited JSON to classic herdr
//!   0.9.x and maps its workspaces, tabs and panes onto terminals.
//!
//! herdr-app is a client of herdr-core's labels and documents, not a terminal
//! backend of its own, so it has no implementation here.
//!
//! Both are projected into the common model in [`model`]. Detected versions
//! outside the verified series degrade to read-only: reads keep working, and
//! every mutation fails with [`error::TerminalError::ReadOnly`].
//!
//! # Safety
//!
//! herdr-core's `terminal.read` with `lines > 0` scrolls a full-screen program
//! and therefore counts as input. Dandelion keeps reads of terminals it did not
//! create at `lines: 0`. Use [`model::AttachMode::Observe`] for live views of
//! those terminals as well.

#![deny(missing_docs)]

pub mod backend;
pub mod detect;
pub mod error;
pub mod events;
pub mod frame;
pub mod herdr;
pub mod herdr_core;
pub mod jsonrpc;
pub mod model;
pub mod stream;

mod util;

#[cfg(test)]
mod test_support;

pub use backend::TerminalBackend;
pub use error::TerminalError;
pub use events::{BackendEvent, EventGate, Resync, TerminalChange, TerminalEvent};
pub use model::{
    Access, AgentKind, AgentState, AgentStatus, AttachMode, AttachOptions, BackendDescriptor,
    BackendKind, BlockedReason, InterruptOutcome, InterruptResult, KeyName, ProtocolId,
    ReadOptions, ReadOrigin, ScreenText, SendOptions, SendOutcome, SpawnSpec, TerminalFilter,
    TerminalId, TerminalInfo, TerminalLifecycle, TerminalSize,
};
pub use stream::{AttachEvent, AttachSession, EventStream};

/// The version of this crate.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
