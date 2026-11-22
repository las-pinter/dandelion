//! The trait every terminal backend implements.

use async_trait::async_trait;

use crate::error::TerminalError;
use crate::model::{
    AttachOptions, BackendDescriptor, BackendKind, InterruptOutcome, KeyName, ReadOptions,
    ScreenText, SendOptions, SendOutcome, SpawnSpec, TerminalFilter, TerminalId, TerminalInfo,
};
use crate::stream::{AttachSession, EventStream};

/// A source of terminals and the agents running in them.
///
/// All reads are safe. Writes fail with [`TerminalError::ReadOnly`] when
/// detection found an unknown version or protocol.
#[async_trait]
pub trait TerminalBackend: Send + Sync {
    /// Which backend this is.
    fn kind(&self) -> BackendKind;

    /// Checks availability, version and protocol.
    ///
    /// A backend that cannot answer is reported as unavailable, not as an
    /// error.
    async fn detect(&self) -> BackendDescriptor;

    /// Lists terminals, optionally filtered by labels.
    ///
    /// Finished terminals are omitted unless the filter asks for them.
    ///
    /// # Errors
    ///
    /// Returns [`TerminalError::Unavailable`] when the backend cannot be
    /// reached and [`TerminalError::Protocol`] for an unintelligible answer.
    async fn list(&self, filter: &TerminalFilter) -> Result<Vec<TerminalInfo>, TerminalError>;

    /// Fetches one terminal.
    ///
    /// # Errors
    ///
    /// Returns [`TerminalError::NotFound`] when the terminal does not exist.
    async fn get(&self, id: &TerminalId) -> Result<TerminalInfo, TerminalError>;

    /// Subscribes to changes.
    ///
    /// The first item is a resync snapshot, and another follows after the
    /// backend reports missed events.
    ///
    /// # Errors
    ///
    /// Returns [`TerminalError::Unavailable`] when the backend cannot be
    /// reached.
    async fn subscribe(&self, filter: &TerminalFilter) -> Result<EventStream, TerminalError>;

    /// Reads the screen as plain text.
    ///
    /// Keep `lines` at zero for terminals Dandelion did not create: a herdr-core
    /// read with history scrolls a full-screen program and counts as input.
    ///
    /// # Errors
    ///
    /// Returns [`TerminalError::NotFound`] when the terminal does not exist.
    async fn read(
        &self,
        id: &TerminalId,
        options: ReadOptions,
    ) -> Result<ScreenText, TerminalError>;

    /// Pastes text and presses Enter.
    ///
    /// # Errors
    ///
    /// Returns [`TerminalError::Blocked`] when the program waits for a person
    /// and `force` is not set.
    async fn send_text(
        &self,
        id: &TerminalId,
        text: &str,
        options: SendOptions,
    ) -> Result<SendOutcome, TerminalError>;

    /// Presses named keys.
    ///
    /// # Errors
    ///
    /// Returns [`TerminalError::InvalidKey`] when the backend rejects a name.
    async fn send_keys(&self, id: &TerminalId, keys: &[KeyName]) -> Result<(), TerminalError>;

    /// Presses the program's stop key.
    ///
    /// # Errors
    ///
    /// Returns [`TerminalError::NotFound`] when the terminal does not exist.
    async fn interrupt(&self, id: &TerminalId) -> Result<InterruptOutcome, TerminalError>;

    /// Ends the program.
    ///
    /// herdr-core keeps the terminal readable as a finished record. Classic
    /// herdr has no finished panes, so its implementation closes the pane and a
    /// later [`TerminalBackend::get`] returns
    /// [`TerminalError::NotFound`](crate::TerminalError::NotFound).
    ///
    /// # Errors
    ///
    /// Returns [`TerminalError::ReadOnly`] for a read-only backend.
    async fn kill(&self, id: &TerminalId) -> Result<(), TerminalError>;

    /// Removes a finished terminal.
    ///
    /// Classic herdr shows only live panes, so its implementation closes an
    /// existing pane and succeeds when the pane is already gone.
    ///
    /// # Errors
    ///
    /// Returns [`TerminalError::StillLive`] when its program still runs.
    async fn remove(&self, id: &TerminalId) -> Result<(), TerminalError>;

    /// Starts a terminal.
    ///
    /// Classic herdr creates a workspace with a root pane; it ignores the
    /// spec's `labels`, `size`, `keep`, `no_parent` and `no_notify` fields.
    ///
    /// # Errors
    ///
    /// Returns [`TerminalError::ReadOnly`] for a read-only backend.
    async fn spawn(&self, spec: &SpawnSpec) -> Result<TerminalInfo, TerminalError>;

    /// Attaches a byte stream.
    ///
    /// # Errors
    ///
    /// Returns [`TerminalError::Unsupported`] when the backend has no stream.
    async fn attach(
        &self,
        id: &TerminalId,
        options: AttachOptions,
    ) -> Result<AttachSession, TerminalError>;
}
