//! Streams returned by backend subscriptions and attaches.

use tokio::sync::mpsc;

use crate::error::TerminalError;
use crate::events::BackendEvent;
use crate::model::TerminalSize;

/// A live backend event stream.
///
/// The first item is always [`BackendEvent::Resync`] with the snapshot the
/// subscription started from.
pub struct EventStream {
    receiver: mpsc::Receiver<Result<BackendEvent, TerminalError>>,
}

impl EventStream {
    /// Wraps the receiving half of a backend's event channel.
    pub(crate) fn new(receiver: mpsc::Receiver<Result<BackendEvent, TerminalError>>) -> Self {
        Self { receiver }
    }

    /// Waits for the next event.
    ///
    /// Returns `None` when the backend closed the subscription.
    pub async fn next(&mut self) -> Option<Result<BackendEvent, TerminalError>> {
        self.receiver.recv().await
    }
}

impl std::fmt::Debug for EventStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventStream").finish_non_exhaustive()
    }
}

/// One item from an attach stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttachEvent {
    /// Terminal output, including the first full repaint.
    Data(Vec<u8>),
    /// The terminal's size, reported by the backend.
    Resized(TerminalSize),
    /// The stream ended.
    Closed {
        /// Why it ended, when the backend said.
        reason: Option<String>,
    },
}

/// A byte stream attached to one terminal.
///
/// The session owns credit accounting: CREDIT is returned as each DATA frame is
/// handed to the bounded event channel, and the channel provides backpressure,
/// so a slow consumer slows the terminal down instead of growing a buffer.
pub struct AttachSession {
    events: mpsc::Receiver<Result<AttachEvent, TerminalError>>,
    commands: mpsc::Sender<AttachCommand>,
}

impl AttachSession {
    pub(crate) fn new(
        events: mpsc::Receiver<Result<AttachEvent, TerminalError>>,
        commands: mpsc::Sender<AttachCommand>,
    ) -> Self {
        Self { events, commands }
    }

    /// Waits for the next attach event.
    pub async fn next_event(&mut self) -> Option<Result<AttachEvent, TerminalError>> {
        self.events.recv().await
    }

    /// Sends bytes to the terminal.
    ///
    /// # Errors
    ///
    /// Returns [`TerminalError::Closed`] when the stream already ended.
    pub async fn input(&self, bytes: Vec<u8>) -> Result<(), TerminalError> {
        self.send(AttachCommand::Input(bytes)).await
    }

    /// Resizes the real terminal. Harmless in observe mode.
    ///
    /// # Errors
    ///
    /// Returns [`TerminalError::Closed`] when the stream already ended.
    pub async fn resize(&self, size: TerminalSize) -> Result<(), TerminalError> {
        self.send(AttachCommand::Resize(size)).await
    }

    /// Detaches the stream.
    ///
    /// # Errors
    ///
    /// Returns [`TerminalError::Closed`] when the stream already ended.
    pub async fn close(&self) -> Result<(), TerminalError> {
        self.send(AttachCommand::Close).await
    }

    async fn send(&self, command: AttachCommand) -> Result<(), TerminalError> {
        self.commands
            .send(command)
            .await
            .map_err(|_| TerminalError::Closed)
    }
}

impl std::fmt::Debug for AttachSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AttachSession").finish_non_exhaustive()
    }
}

/// Commands the attach task understands.
#[derive(Debug)]
pub(crate) enum AttachCommand {
    Input(Vec<u8>),
    Resize(TerminalSize),
    Close,
}
