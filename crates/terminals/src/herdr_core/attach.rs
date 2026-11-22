//! herdr-core framed attach: handshake, frame relay and CREDIT flow control.

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tokio::time::timeout;

use crate::error::TerminalError;
use crate::frame::{Frame, FrameDecoder, FrameKind};
use crate::jsonrpc::{self, Incoming, Request};
use crate::model::{AttachMode, AttachOptions, TerminalId};
use crate::stream::{AttachCommand, AttachEvent, AttachSession};
use crate::util::CALL_TIMEOUT;

use super::client::CoreClient;
use super::wire::{AttachParams, AttachResult};

/// Channel carrying JSON-RPC control messages.
const CONTROL_CHANNEL: u32 = 0;

/// How many output frames may wait for the consumer.
const EVENT_CAPACITY: usize = 128;

/// How many commands may wait for the socket.
const COMMAND_CAPACITY: usize = 32;

/// Opens a framed connection, attaches to a terminal, and returns its session.
pub(crate) async fn attach(
    client: &CoreClient,
    id: &TerminalId,
    options: AttachOptions,
) -> Result<AttachSession, TerminalError> {
    let mut stream = client.connect().await?;

    let hello = Request::new(
        "hello",
        "hello",
        serde_json::json!({
            "generation": crate::detect::CORE_GENERATION,
            "framed": true,
        }),
    );
    stream.write_all(hello.encode_line().as_bytes()).await?;
    stream.flush().await?;

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let read = timeout(CALL_TIMEOUT, reader.read_line(&mut line))
        .await
        .map_err(|_| TerminalError::Timeout)??;
    if read == 0 {
        return Err(TerminalError::Protocol(
            "core closed the connection during the framed hello".into(),
        ));
    }
    match jsonrpc::parse_line(&line)? {
        Incoming::Response(response) => {
            let hello = response.into_result()?;
            if hello.get("framed").and_then(Value::as_bool) != Some(true) {
                return Err(TerminalError::Protocol(
                    "core did not switch to framed mode".into(),
                ));
            }
        }
        Incoming::Notification(notification) => {
            return Err(TerminalError::Protocol(format!(
                "unexpected notification {} during the framed hello",
                notification.method
            )));
        }
    }

    let mut stream = reader.into_inner();
    let params = AttachParams {
        terminal: id.as_str().to_owned(),
        cols: options.size.cols,
        rows: options.size.rows,
        observe: options.mode == AttachMode::Observe,
        history: options.history,
        resize_notifications: true,
    };
    let request = Request::new(1_u64, "terminal.attach", serde_json::to_value(params)?);
    let frame = Frame::data(CONTROL_CHANNEL, request.encode_line().into_bytes());
    write_frame(&mut stream, frame).await?;

    let mut decoder = FrameDecoder::new();
    let mut buffer = [0_u8; 8192];
    let result: AttachResult = loop {
        let read = timeout(CALL_TIMEOUT, stream.read(&mut buffer))
            .await
            .map_err(|_| TerminalError::Timeout)??;
        if read == 0 {
            return Err(TerminalError::Protocol(
                "core closed the connection before answering terminal.attach".into(),
            ));
        }
        decoder.push(&buffer[..read]);
        let mut completed = None;
        while let Some(frame) = decoder.next_frame()? {
            if frame.channel != CONTROL_CHANNEL || frame.kind != FrameKind::Data {
                continue;
            }
            let payload = String::from_utf8(frame.payload)
                .map_err(|error| TerminalError::Protocol(error.to_string()))?;
            if let Incoming::Response(response) = jsonrpc::parse_line(&payload)? {
                let value = response.into_result()?;
                completed = Some(serde_json::from_value(value)?);
                break;
            }
        }
        if let Some(result) = completed {
            break result;
        }
    };

    let (event_sender, event_receiver) = mpsc::channel(EVENT_CAPACITY);
    let (command_sender, command_receiver) = mpsc::channel(COMMAND_CAPACITY);
    tokio::spawn(run_stream(
        stream,
        decoder,
        result.channel,
        event_sender,
        command_receiver,
    ));
    Ok(AttachSession::new(event_receiver, command_sender))
}

/// Relays frames between the core and the session channels.
///
/// Generic over the stream so a unit test can use a socket pair instead of a
/// real core.
pub(crate) async fn run_stream<S>(
    mut stream: S,
    mut decoder: FrameDecoder,
    channel: u32,
    events: mpsc::Sender<Result<AttachEvent, TerminalError>>,
    mut commands: mpsc::Receiver<AttachCommand>,
) -> Result<(), TerminalError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    // The handshake may already have buffered output frames after the attach
    // reply; forward them before waiting for new bytes.
    if drain_frames(&mut stream, &mut decoder, channel, &events).await? {
        return Ok(());
    }
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        tokio::select! {
            command = commands.recv() => {
                match command {
                    Some(AttachCommand::Input(bytes)) => {
                        write_frame(&mut stream, Frame::data(channel, bytes)).await?;
                    }
                    Some(AttachCommand::Resize(size)) => {
                        write_frame(&mut stream, Frame::resize(channel, size)).await?;
                    }
                    Some(AttachCommand::Close) | None => {
                        let _ = write_frame(&mut stream, Frame::close(channel)).await;
                        let _ = events
                            .send(Ok(AttachEvent::Closed { reason: None }))
                            .await;
                        return Ok(());
                    }
                }
            }
            read = stream.read(&mut buffer) => {
                let read = read?;
                if read == 0 {
                    let _ = events
                        .send(Ok(AttachEvent::Closed {
                            reason: Some("core closed the stream".to_owned()),
                        }))
                        .await;
                    return Ok(());
                }
                decoder.push(&buffer[..read]);
                if drain_frames(&mut stream, &mut decoder, channel, &events).await? {
                    return Ok(());
                }
            }
        }
    }
}

/// Forwards every complete buffered frame. Returns true when the relay should
/// stop because the core closed the stream or the consumer went away.
async fn drain_frames<S>(
    stream: &mut S,
    decoder: &mut FrameDecoder,
    channel: u32,
    events: &mpsc::Sender<Result<AttachEvent, TerminalError>>,
) -> Result<bool, TerminalError>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    while let Some(frame) = decoder.next_frame()? {
        if frame.channel != channel {
            continue;
        }
        match frame.kind {
            FrameKind::Data => {
                let consumed = u32::try_from(frame.payload.len())
                    .map_err(|_| TerminalError::Protocol("frame too large".into()))?;
                if events
                    .send(Ok(AttachEvent::Data(frame.payload)))
                    .await
                    .is_err()
                {
                    return Ok(true);
                }
                write_frame(stream, Frame::credit(channel, consumed)).await?;
            }
            FrameKind::Resize => {
                if let Some(size) = frame.resize_size()
                    && events.send(Ok(AttachEvent::Resized(size))).await.is_err()
                {
                    return Ok(true);
                }
            }
            FrameKind::Close => {
                let _ = events.send(Ok(AttachEvent::Closed { reason: None })).await;
                return Ok(true);
            }
            _ => {}
        }
    }
    Ok(false)
}

async fn write_frame<S>(stream: &mut S, frame: Frame) -> Result<(), TerminalError>
where
    S: AsyncWrite + Unpin,
{
    let bytes = frame
        .encode()
        .map_err(|error| TerminalError::Protocol(error.to_string()))?;
    stream.write_all(&bytes).await?;
    stream.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;
    use tokio::sync::mpsc;
    use tokio::time::timeout;

    use super::run_stream;
    use crate::frame::{Frame, FrameDecoder, FrameKind};
    use crate::model::TerminalSize;
    use crate::stream::{AttachCommand, AttachEvent};

    async fn read_frame(stream: &mut UnixStream) -> Frame {
        let mut header = [0_u8; 9];
        stream.read_exact(&mut header).await.expect("header");
        let length = u32::from_le_bytes([header[5], header[6], header[7], header[8]]) as usize;
        let mut payload = vec![0_u8; length];
        stream.read_exact(&mut payload).await.expect("payload");
        Frame {
            channel: u32::from_le_bytes([header[0], header[1], header[2], header[3]]),
            kind: FrameKind::from_byte(header[4]),
            payload,
        }
    }

    async fn write_frame(stream: &mut UnixStream, frame: Frame) {
        stream
            .write_all(&frame.encode().expect("encode"))
            .await
            .expect("write");
    }

    #[tokio::test]
    async fn data_is_forwarded_and_credited() {
        let (client, mut core) = UnixStream::pair().expect("pair");
        let (events, mut received) = mpsc::channel(8);
        let (commands, command_receiver) = mpsc::channel(8);
        let task = tokio::spawn(run_stream(
            client,
            FrameDecoder::new(),
            7,
            events,
            command_receiver,
        ));

        write_frame(&mut core, Frame::data(7, b"hello".to_vec())).await;
        let event = timeout(Duration::from_secs(2), received.recv())
            .await
            .expect("timeout")
            .expect("event");
        assert_eq!(event.expect("ok"), AttachEvent::Data(b"hello".to_vec()));

        let credit = timeout(Duration::from_secs(2), read_frame(&mut core))
            .await
            .expect("credit timeout");
        assert_eq!(credit.kind, FrameKind::Credit);
        assert_eq!(credit.credit_bytes(), Some(5));

        drop(commands);
        assert!(
            timeout(Duration::from_secs(2), task)
                .await
                .expect("task")
                .is_ok()
        );
    }

    #[tokio::test]
    async fn frames_buffered_by_the_handshake_are_delivered_first() {
        let (client, mut core) = UnixStream::pair().expect("pair");
        let mut decoder = FrameDecoder::new();
        decoder.push(&Frame::data(5, b"early".to_vec()).encode().expect("encode"));
        let (events, mut received) = mpsc::channel(8);
        let (commands, command_receiver) = mpsc::channel(8);
        let task = tokio::spawn(run_stream(client, decoder, 5, events, command_receiver));

        let event = timeout(Duration::from_secs(2), received.recv())
            .await
            .expect("timeout")
            .expect("event")
            .expect("ok");
        assert_eq!(event, AttachEvent::Data(b"early".to_vec()));

        let credit = timeout(Duration::from_secs(2), read_frame(&mut core))
            .await
            .expect("credit timeout");
        assert_eq!(credit.credit_bytes(), Some(5));

        drop(commands);
        assert!(
            timeout(Duration::from_secs(2), task)
                .await
                .expect("task")
                .is_ok()
        );
    }

    #[tokio::test]
    async fn resize_and_close_are_reported_and_input_is_forwarded() {
        let (client, mut core) = UnixStream::pair().expect("pair");
        let (events, mut received) = mpsc::channel(8);
        let (commands, command_receiver) = mpsc::channel(8);
        let task = tokio::spawn(run_stream(
            client,
            FrameDecoder::new(),
            3,
            events,
            command_receiver,
        ));

        write_frame(&mut core, Frame::resize(3, TerminalSize::new(100, 30))).await;
        let event = timeout(Duration::from_secs(2), received.recv())
            .await
            .expect("timeout")
            .expect("event")
            .expect("ok");
        assert_eq!(event, AttachEvent::Resized(TerminalSize::new(100, 30)));

        commands
            .send(AttachCommand::Input(b"typed".to_vec()))
            .await
            .expect("send input");
        let input = timeout(Duration::from_secs(2), read_frame(&mut core))
            .await
            .expect("input timeout");
        assert_eq!(input.channel, 3);
        assert_eq!(input.kind, FrameKind::Data);
        assert_eq!(input.payload, b"typed");

        write_frame(&mut core, Frame::close(3)).await;
        let event = timeout(Duration::from_secs(2), received.recv())
            .await
            .expect("timeout")
            .expect("event")
            .expect("ok");
        assert_eq!(event, AttachEvent::Closed { reason: None });
        assert!(
            timeout(Duration::from_secs(2), task)
                .await
                .expect("task")
                .is_ok()
        );
    }
}
