//! The binary attach framing herdr-core uses in framed mode.
//!
//! A frame is `channel u32 LE | kind u8 | length u32 LE | payload`. Channel 0
//! carries JSON-RPC; the channel returned by `terminal.attach` carries terminal
//! output and control frames.

use thiserror::Error;

use crate::error::TerminalError;
use crate::model::TerminalSize;

/// Bytes in a frame header.
pub const HEADER_LEN: usize = 9;

/// Largest payload accepted from a peer.
pub const MAX_PAYLOAD: u32 = 16 * 1024 * 1024;

/// Frame kinds herdr-core defines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameKind {
    /// Terminal output or input bytes.
    Data,
    /// Bytes consumed by the receiver of the opposite direction.
    Credit,
    /// The stream ended.
    Close,
    /// `cols u16 LE, rows u16 LE`.
    Resize,
    /// Focus change (payload not verified).
    Focus,
    /// Terminal facts (payload not verified).
    Facts,
    /// Automatic input acknowledgement.
    AutomaticInput,
    /// A kind this version does not know.
    Other(u8),
}

impl FrameKind {
    /// Decodes the wire byte.
    #[must_use]
    pub const fn from_byte(byte: u8) -> Self {
        match byte {
            0 => Self::Data,
            1 => Self::Credit,
            2 => Self::Close,
            3 => Self::Resize,
            4 => Self::Focus,
            5 => Self::Facts,
            6 => Self::AutomaticInput,
            other => Self::Other(other),
        }
    }

    /// Encodes the wire byte.
    #[must_use]
    pub const fn to_byte(self) -> u8 {
        match self {
            Self::Data => 0,
            Self::Credit => 1,
            Self::Close => 2,
            Self::Resize => 3,
            Self::Focus => 4,
            Self::Facts => 5,
            Self::AutomaticInput => 6,
            Self::Other(other) => other,
        }
    }
}

/// A decoding failure that makes the rest of the stream meaningless.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum FrameError {
    /// The peer announced a payload larger than [`MAX_PAYLOAD`].
    #[error("frame payload of {length} bytes exceeds the 16 MiB limit")]
    TooLarge {
        /// Announced length.
        length: u32,
    },
    /// A payload was too large to encode.
    #[error("frame payload is too large to encode")]
    PayloadTooLarge,
}

impl From<FrameError> for TerminalError {
    fn from(error: FrameError) -> Self {
        Self::Protocol(error.to_string())
    }
}

/// One frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Multiplexed stream channel.
    pub channel: u32,
    /// What the payload means.
    pub kind: FrameKind,
    /// Frame payload.
    pub payload: Vec<u8>,
}

impl Frame {
    /// Builds a DATA frame.
    #[must_use]
    pub fn data(channel: u32, payload: impl Into<Vec<u8>>) -> Self {
        Self {
            channel,
            kind: FrameKind::Data,
            payload: payload.into(),
        }
    }

    /// Builds a CREDIT frame for a number of consumed bytes.
    #[must_use]
    pub fn credit(channel: u32, consumed: u32) -> Self {
        Self {
            channel,
            kind: FrameKind::Credit,
            payload: consumed.to_le_bytes().to_vec(),
        }
    }

    /// Builds a CLOSE frame.
    #[must_use]
    pub fn close(channel: u32) -> Self {
        Self {
            channel,
            kind: FrameKind::Close,
            payload: Vec::new(),
        }
    }

    /// Builds a RESIZE frame.
    #[must_use]
    pub fn resize(channel: u32, size: TerminalSize) -> Self {
        let mut payload = Vec::with_capacity(4);
        payload.extend_from_slice(&size.cols.to_le_bytes());
        payload.extend_from_slice(&size.rows.to_le_bytes());
        Self {
            channel,
            kind: FrameKind::Resize,
            payload,
        }
    }

    /// Encodes the frame, header included.
    ///
    /// # Errors
    ///
    /// Returns [`FrameError::PayloadTooLarge`] when the payload does not fit in
    /// the protocol's length field.
    pub fn encode(&self) -> Result<Vec<u8>, FrameError> {
        let length = u32::try_from(self.payload.len()).map_err(|_| FrameError::PayloadTooLarge)?;
        let mut bytes = Vec::with_capacity(HEADER_LEN + self.payload.len());
        bytes.extend_from_slice(&self.channel.to_le_bytes());
        bytes.push(self.kind.to_byte());
        bytes.extend_from_slice(&length.to_le_bytes());
        bytes.extend_from_slice(&self.payload);
        Ok(bytes)
    }

    /// Reads the resize payload, when this is a valid RESIZE frame.
    #[must_use]
    pub fn resize_size(&self) -> Option<TerminalSize> {
        if self.kind != FrameKind::Resize || self.payload.len() < 4 {
            return None;
        }
        let cols = u16::from_le_bytes([self.payload[0], self.payload[1]]);
        let rows = u16::from_le_bytes([self.payload[2], self.payload[3]]);
        Some(TerminalSize::new(cols, rows))
    }

    /// Reads the consumed-byte count, when this is a valid CREDIT frame.
    #[must_use]
    pub fn credit_bytes(&self) -> Option<u32> {
        if self.kind != FrameKind::Credit || self.payload.len() < 4 {
            return None;
        }
        Some(u32::from_le_bytes([
            self.payload[0],
            self.payload[1],
            self.payload[2],
            self.payload[3],
        ]))
    }
}

/// Buffers bytes from a stream and yields complete frames.
#[derive(Debug, Default)]
pub struct FrameDecoder {
    buffer: Vec<u8>,
}

impl FrameDecoder {
    /// A decoder with an empty buffer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds raw bytes.
    pub fn push(&mut self, bytes: &[u8]) {
        self.buffer.extend_from_slice(bytes);
    }

    /// Number of buffered bytes not yet forming a complete frame.
    #[must_use]
    pub fn buffered(&self) -> usize {
        self.buffer.len()
    }

    /// Pops the next complete frame.
    ///
    /// Returns `Ok(None)` while the buffered bytes are not yet a whole frame.
    ///
    /// # Errors
    ///
    /// Returns [`FrameError::TooLarge`] when the announced payload is over the
    /// limit. The stream is unusable after that.
    pub fn next_frame(&mut self) -> Result<Option<Frame>, FrameError> {
        if self.buffer.len() < HEADER_LEN {
            return Ok(None);
        }
        let channel = u32::from_le_bytes([
            self.buffer[0],
            self.buffer[1],
            self.buffer[2],
            self.buffer[3],
        ]);
        let kind = FrameKind::from_byte(self.buffer[4]);
        let length = u32::from_le_bytes([
            self.buffer[5],
            self.buffer[6],
            self.buffer[7],
            self.buffer[8],
        ]);
        if length > MAX_PAYLOAD {
            return Err(FrameError::TooLarge { length });
        }
        let total = HEADER_LEN
            .checked_add(length as usize)
            .ok_or(FrameError::TooLarge { length })?;
        if self.buffer.len() < total {
            return Ok(None);
        }
        let payload = self.buffer[HEADER_LEN..total].to_vec();
        self.buffer.drain(..total);
        Ok(Some(Frame {
            channel,
            kind,
            payload,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::{Frame, FrameDecoder, FrameError, FrameKind, HEADER_LEN, MAX_PAYLOAD};
    use crate::model::TerminalSize;

    #[test]
    fn data_frame_has_the_verified_header_layout() {
        let encoded = Frame::data(1, b"hi".to_vec()).encode().expect("encode");
        assert_eq!(encoded, vec![1, 0, 0, 0, 0, 2, 0, 0, 0, b'h', b'i']);
        assert_eq!(encoded.len(), HEADER_LEN + 2);
    }

    #[test]
    fn credit_is_a_little_endian_consumed_count() {
        let encoded = Frame::credit(1, 262_144).encode().expect("encode");
        assert_eq!(encoded[0..4], [1, 0, 0, 0]);
        assert_eq!(encoded[4], 1);
        assert_eq!(encoded[5..9], [4, 0, 0, 0]);
        assert_eq!(encoded[9..], [0, 0, 4, 0]);
    }

    #[test]
    fn resize_round_trips_columns_and_rows() {
        let frame = Frame::resize(2, TerminalSize::new(100, 30));
        assert_eq!(frame.payload, vec![100, 0, 30, 0]);
        assert_eq!(frame.resize_size(), Some(TerminalSize::new(100, 30)));
    }

    #[test]
    fn decoder_handles_split_headers_and_several_frames() {
        let first = Frame::data(1, b"first".to_vec()).encode().expect("encode");
        let second = Frame::credit(1, 5).encode().expect("encode");
        let third = Frame::close(1).encode().expect("encode");
        let first_len = first.len();
        let mut all = Vec::new();
        all.extend_from_slice(&first);
        all.extend_from_slice(&second);
        all.extend_from_slice(&third);

        let mut decoder = FrameDecoder::new();
        decoder.push(&all[..3]);
        assert!(decoder.next_frame().expect("partial").is_none());
        decoder.push(&all[3..first_len]);
        let frame = decoder.next_frame().expect("frame").expect("data");
        assert_eq!(frame.channel, 1);
        assert_eq!(frame.payload, b"first");
        decoder.push(&all[first_len..first_len + 8]);
        assert!(decoder.next_frame().expect("partial credit").is_none());
        decoder.push(&all[first_len + 8..]);
        assert!(decoder.next_frame().expect("credit").is_some());
        let close = decoder.next_frame().expect("close").expect("frame");
        assert_eq!(close.kind, FrameKind::Close);
        assert_eq!(close.payload.len(), 0);
        assert_eq!(decoder.buffered(), 0);
    }

    #[test]
    fn recorded_attach_frames_decode_and_reencode_byte_for_byte() {
        let raw = crate::test_support::core_attach_frames();
        assert_eq!(raw.len(), 5);
        let mut decoder = FrameDecoder::new();
        let mut frames = Vec::new();
        for bytes in &raw {
            decoder.push(bytes);
            while let Some(frame) = decoder.next_frame().expect("decode") {
                frames.push(frame.encode().expect("encode"));
            }
        }
        assert_eq!(frames, raw);

        let resize = &raw[1];
        let mut decoder = FrameDecoder::new();
        decoder.push(resize);
        let frame = decoder.next_frame().expect("resize").expect("frame");
        assert_eq!(frame.channel, 1);
        assert_eq!(frame.kind, FrameKind::Resize);
        assert_eq!(frame.resize_size(), Some(TerminalSize::new(200, 120)));

        let mut decoder = FrameDecoder::new();
        decoder.push(&raw[3]);
        let credit = decoder.next_frame().expect("credit").expect("frame");
        assert_eq!(credit.credit_bytes(), Some(30));

        let mut decoder = FrameDecoder::new();
        decoder.push(&raw[4]);
        let close = decoder.next_frame().expect("close").expect("frame");
        assert_eq!(close.kind, FrameKind::Close);
        assert_eq!(close.payload.len(), 0);
    }

    #[test]
    fn oversized_payloads_are_rejected_before_buffering_them() {
        let mut decoder = FrameDecoder::new();
        let mut header = Vec::new();
        header.extend_from_slice(&1_u32.to_le_bytes());
        header.push(FrameKind::Data.to_byte());
        header.extend_from_slice(&(MAX_PAYLOAD + 1).to_le_bytes());
        decoder.push(&header);

        assert_eq!(
            decoder.next_frame(),
            Err(FrameError::TooLarge {
                length: MAX_PAYLOAD + 1
            })
        );
    }
}
