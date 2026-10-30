//! Shared, nonblocking client-side protocol session for interactive clients.
//!
//! [`ClientSession`] owns the socket and bounded wire buffers, but deliberately
//! does not own a thread, event loop, or process. Call [`ClientSession::poll`]
//! from the host's loop and pass a reusable event vector to avoid allocating an
//! event collection on each frame. Events are appended; clear the vector before
//! polling if it should contain only the latest poll's events.

use minimium_protocol::{
    decode_server, encode_client, ClientMessage, ProtocolError, ServerMessage, MAX_FRAME_BYTES,
};
use std::{
    io::{self, Read, Write},
    net::TcpStream,
    time::{Duration, Instant},
};

/// Maximum retained receive or pending send data.
pub const MAX_SESSION_BUFFER_BYTES: usize = 64 * 1024;
/// Maximum amount read in a single poll, keeping UI loops responsive.
pub const MAX_SESSION_READ_BYTES_PER_POLL: usize = 16 * 1024;
pub const WELCOME_TIMEOUT: Duration = Duration::from_secs(5);
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
pub const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionState {
    Disconnected,
    AwaitingWelcome,
    Connected,
}

/// A terminal reason for the current connection. The session is reset to
/// [`SessionState::Disconnected`] when one of these is emitted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DisconnectReason {
    PeerClosed,
    ServerClosed,
    WelcomeTimeout,
    HeartbeatTimeout,
    Protocol(ProtocolError),
    UnexpectedMessage,
    Rejected(minimium_protocol::ServerErrorCode),
    MapMismatch { expected: u64, actual: u64 },
    Io(io::ErrorKind),
    BufferLimit,
}

/// Typed output from a session poll. Every valid server message is delivered
/// as `Message`; a matching Pong also produces `PingRoundTrip`.
#[derive(Clone, Debug, PartialEq)]
pub enum SessionEvent {
    Message(ServerMessage),
    PingRoundTrip(Duration),
    Disconnected(DisconnectReason),
}

/// Cumulative wire traffic metrics and the most recent measured ping RTT.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SessionMetrics {
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub ping_round_trip: Option<Duration>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueueError {
    Disconnected,
    AwaitingWelcome,
    IdentifyReserved,
    BufferFull,
    Protocol(ProtocolError),
}

/// A single nonblocking TCP protocol session.
pub struct ClientSession {
    stream: Option<TcpStream>,
    state: SessionState,
    expected_map_fingerprint: u64,
    state_since: Instant,
    last_heartbeat_at: Instant,
    pending_ping_at: Option<Instant>,
    pending_ping_end: Option<usize>,
    receive_buffer: Vec<u8>,
    send_buffer: Vec<u8>,
    send_offset: usize,
    metrics: SessionMetrics,
}

impl ClientSession {
    /// Starts a fresh session, sets the supplied stream nonblocking, and queues
    /// the required Identify handshake frame. The first poll writes it.
    pub fn new(
        stream: TcpStream,
        profile_id: PlayerProfileId,
        expected_map_fingerprint: u64,
    ) -> io::Result<Self> {
        stream.set_nonblocking(true)?;
        stream.set_nodelay(true)?;
        let now = Instant::now();
        let mut session = Self {
            stream: Some(stream),
            state: SessionState::AwaitingWelcome,
            expected_map_fingerprint,
            state_since: now,
            last_heartbeat_at: now,
            pending_ping_at: None,
            pending_ping_end: None,
            receive_buffer: Vec::with_capacity(MAX_SESSION_BUFFER_BYTES),
            send_buffer: Vec::with_capacity(MAX_SESSION_BUFFER_BYTES),
            send_offset: 0,
            metrics: SessionMetrics::default(),
        };
        session
            .append_encoded(&ClientMessage::Identify(profile_id))
            .expect("Identify frame is within protocol limits");
        Ok(session)
    }

    pub fn state(&self) -> SessionState {
        self.state
    }

    pub fn metrics(&self) -> SessionMetrics {
        self.metrics
    }

    /// Close the current socket and discard all buffered data.
    pub fn close(&mut self) {
        self.stream = None;
        self.state = SessionState::Disconnected;
        self.pending_ping_at = None;
        self.pending_ping_end = None;
        self.receive_buffer.clear();
        self.send_buffer.clear();
        self.send_offset = 0;
    }

    /// Queues an application message. Identify is reserved for construction;
    /// all other messages are accepted only after Welcome. A full bounded send
    /// buffer is reported without disconnecting so the caller can retry.
    pub fn queue(&mut self, message: &ClientMessage) -> Result<(), QueueError> {
        match self.state {
            SessionState::Disconnected => return Err(QueueError::Disconnected),
            SessionState::AwaitingWelcome => return Err(QueueError::AwaitingWelcome),
            SessionState::Connected => {}
        }
        if matches!(message, ClientMessage::Identify(_)) {
            return Err(QueueError::IdentifyReserved);
        }
        let frame = encode_client(message).map_err(QueueError::Protocol)?;
        self.append_frame(&frame)
            .map_err(|()| QueueError::BufferFull)?;
        if matches!(message, ClientMessage::Ping)
            && self.pending_ping_at.is_none()
            && self.pending_ping_end.is_none()
        {
            self.pending_ping_end = Some(self.send_buffer.len());
        }
        Ok(())
    }

    /// Performs bounded nonblocking reads, flushes queued writes (including
    /// partial writes), validates protocol state, and maintains heartbeat
    /// timers. Appends events to caller-owned storage and returns the current
    /// state. Fatal conditions append one Disconnected event and clear all
    /// buffers and connection state.
    pub fn poll(&mut self, events: &mut Vec<SessionEvent>) -> SessionState {
        let now = Instant::now();
        if self.state == SessionState::AwaitingWelcome
            && now.saturating_duration_since(self.state_since) >= WELCOME_TIMEOUT
        {
            self.disconnect(DisconnectReason::WelcomeTimeout, events);
            return self.state;
        }
        if self.state == SessionState::Connected
            && self
                .pending_ping_at
                .is_some_and(|sent| now.saturating_duration_since(sent) >= HEARTBEAT_TIMEOUT)
        {
            self.disconnect(DisconnectReason::HeartbeatTimeout, events);
            return self.state;
        }

        self.flush_send(events);
        if self.state == SessionState::Disconnected {
            return self.state;
        }

        self.read_available(events);
        if self.state == SessionState::Disconnected {
            return self.state;
        }
        self.decode_available(events);
        if self.state == SessionState::Disconnected {
            return self.state;
        }

        let now = Instant::now();
        if self.state == SessionState::Connected
            && self.pending_ping_at.is_none()
            && self.pending_ping_end.is_none()
            && now.saturating_duration_since(self.last_heartbeat_at) >= HEARTBEAT_INTERVAL
            && self.queue(&ClientMessage::Ping).is_ok()
        {
            self.last_heartbeat_at = now;
        }
        self.flush_send(events);
        self.state
    }

    fn append_encoded(&mut self, message: &ClientMessage) -> Result<(), ProtocolError> {
        let frame = encode_client(message)?;
        self.append_frame(&frame)
            .map_err(|()| ProtocolError::PayloadTooLarge)
    }

    fn append_frame(&mut self, frame: &[u8]) -> Result<(), ()> {
        if self.send_offset > 0 {
            if let Some(end) = &mut self.pending_ping_end {
                *end = end.saturating_sub(self.send_offset);
            }
            self.send_buffer.drain(..self.send_offset);
            self.send_offset = 0;
        }
        if self.send_buffer.len().saturating_add(frame.len()) > MAX_SESSION_BUFFER_BYTES {
            return Err(());
        }
        self.send_buffer.extend_from_slice(frame);
        Ok(())
    }

    fn flush_send(&mut self, events: &mut Vec<SessionEvent>) {
        while self.send_offset < self.send_buffer.len() {
            let result = match self.stream.as_mut() {
                Some(stream) => stream.write(&self.send_buffer[self.send_offset..]),
                None => return,
            };
            match result {
                Ok(0) => {
                    self.disconnect(DisconnectReason::Io(io::ErrorKind::WriteZero), events);
                    return;
                }
                Ok(count) => {
                    self.send_offset += count;
                    self.metrics.bytes_sent = self.metrics.bytes_sent.saturating_add(count as u64);
                    if self
                        .pending_ping_end
                        .is_some_and(|end| self.send_offset >= end)
                    {
                        self.pending_ping_end = None;
                        self.pending_ping_at = Some(Instant::now());
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    self.disconnect(DisconnectReason::Io(error.kind()), events);
                    return;
                }
            }
        }
        if self.send_offset == self.send_buffer.len() {
            self.send_buffer.clear();
            self.send_offset = 0;
        }
    }

    fn read_available(&mut self, events: &mut Vec<SessionEvent>) {
        let mut chunk = [0u8; 4096];
        let mut total_read = 0;
        loop {
            let available = (MAX_SESSION_READ_BYTES_PER_POLL - total_read).min(chunk.len());
            if available == 0 {
                break;
            }
            let result = match self.stream.as_mut() {
                Some(stream) => stream.read(&mut chunk[..available]),
                None => return,
            };
            match result {
                Ok(0) => {
                    self.decode_available(events);
                    if self.state != SessionState::Disconnected {
                        self.disconnect(DisconnectReason::PeerClosed, events);
                    }
                    return;
                }
                Ok(count) => {
                    total_read += count;
                    self.metrics.bytes_received =
                        self.metrics.bytes_received.saturating_add(count as u64);
                    if self.receive_buffer.len().saturating_add(count) > MAX_SESSION_BUFFER_BYTES {
                        self.disconnect(DisconnectReason::BufferLimit, events);
                        return;
                    }
                    self.receive_buffer.extend_from_slice(&chunk[..count]);
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    self.disconnect(DisconnectReason::Io(error.kind()), events);
                    return;
                }
            }
        }
    }

    fn decode_available(&mut self, events: &mut Vec<SessionEvent>) {
        loop {
            match decode_server(&self.receive_buffer) {
                Ok(Some((message, frame_bytes))) => {
                    self.receive_buffer.drain(..frame_bytes);
                    match &message {
                        ServerMessage::Welcome {
                            map_fingerprint, ..
                        } => {
                            if self.state != SessionState::AwaitingWelcome {
                                self.disconnect(DisconnectReason::UnexpectedMessage, events);
                                return;
                            }
                            if *map_fingerprint != self.expected_map_fingerprint {
                                self.disconnect(
                                    DisconnectReason::MapMismatch {
                                        expected: self.expected_map_fingerprint,
                                        actual: *map_fingerprint,
                                    },
                                    events,
                                );
                                return;
                            }
                            self.state = SessionState::Connected;
                            self.state_since = Instant::now();
                            self.last_heartbeat_at = self.state_since;
                        }
                        ServerMessage::Error(code)
                            if self.state == SessionState::AwaitingWelcome =>
                        {
                            let code = *code;
                            self.disconnect(DisconnectReason::Rejected(code), events);
                            return;
                        }
                        ServerMessage::Bye if self.state == SessionState::Connected => {
                            self.disconnect(DisconnectReason::ServerClosed, events);
                            return;
                        }
                        _ if self.state != SessionState::Connected => {
                            self.disconnect(DisconnectReason::UnexpectedMessage, events);
                            return;
                        }
                        _ => {}
                    }
                    if matches!(&message, ServerMessage::Pong) {
                        if let Some(sent_at) = self.pending_ping_at.take() {
                            let rtt = Instant::now().saturating_duration_since(sent_at);
                            self.metrics.ping_round_trip = Some(rtt);
                            events.push(SessionEvent::PingRoundTrip(rtt));
                            continue;
                        }
                    }
                    events.push(SessionEvent::Message(message));
                }
                Ok(None) => {
                    if self.receive_buffer.len() > MAX_FRAME_BYTES {
                        self.disconnect(DisconnectReason::BufferLimit, events);
                    }
                    return;
                }
                Err(error) => {
                    self.disconnect(DisconnectReason::Protocol(error), events);
                    return;
                }
            }
        }
    }

    fn disconnect(&mut self, reason: DisconnectReason, events: &mut Vec<SessionEvent>) {
        self.close();
        events.push(SessionEvent::Disconnected(reason));
    }
}
use minimium_core::identity::PlayerProfileId;
