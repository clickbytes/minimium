//! Bounded client message transport, independent of the logical client session.
mod tcp;

use minimium_protocol::{ClientMessage, ProtocolError, ServerMessage};
use std::{fmt, io, net::SocketAddr, time::Instant};

pub use tcp::TcpClientTransport;

pub const MAX_CLIENT_BUFFER_BYTES: usize = 64 * 1024;
pub const MAX_CLIENT_READ_BYTES_PER_POLL: usize = 16 * 1024;

/// Stable, nonzero frame completion marker, local to one transport lifetime.
/// Alternate adapters must allocate monotonically without reuse or wrapping.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ClientFrameToken(u64);

impl ClientFrameToken {
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 {
            None
        } else {
            Some(Self(value))
        }
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ClientTransportMetrics {
    pub bytes_sent: u64,
    pub bytes_received: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientReadStatus {
    Open,
    /// Complete buffered messages remain available before the owner closes.
    PeerClosed,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ClientFlushProgress {
    /// The supplied host sample when a write completed the tracked frame.
    /// Queue acceptance alone never produces a completion timestamp.
    pub tracked_frame_sent_at: Option<Instant>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientTransportTimeError {
    Backward,
    Overflow,
}

impl fmt::Display for ClientTransportTimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "client transport time: {self:?}")
    }
}
impl std::error::Error for ClientTransportTimeError {}

#[derive(Debug)]
pub enum ClientTransportError {
    Io(io::Error),
    Protocol(ProtocolError),
    BufferLimit,
    Time(ClientTransportTimeError),
}

impl fmt::Display for ClientTransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Time(error) => error.fmt(formatter),
            Self::Io(error) => write!(formatter, "client transport I/O: {error}"),
            Self::Protocol(error) => write!(formatter, "client transport protocol: {error}"),
            Self::BufferLimit => formatter.write_str("client receive buffer limit exceeded"),
        }
    }
}

impl std::error::Error for ClientTransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Time(error) => Some(error),
            Self::Io(error) => Some(error),
            Self::Protocol(error) => Some(error),
            Self::BufferLimit => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClientQueueError {
    Closed,
    BufferFull,
    Protocol(ProtocolError),
    TokenExhausted,
}

impl fmt::Display for ClientQueueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed => formatter.write_str("client transport is closed"),
            Self::BufferFull => formatter.write_str("client send buffer is full"),
            Self::Protocol(error) => write!(formatter, "client transport encoding: {error}"),
            Self::TokenExhausted => formatter.write_str("client frame tokens exhausted"),
        }
    }
}

impl std::error::Error for ClientQueueError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Protocol(error) => Some(error),
            _ => None,
        }
    }
}

/// Message-level, nonblocking transport owned exclusively by one client session.
///
/// Implementations retain bounded inbound/outbound storage and FIFO order.
/// A front message stays charged to inbound capacity until explicitly consumed;
/// peeking must not advance it. This permits session-level deferred transitions
/// without draining arbitrary messages into another queue. All partial frames,
/// wire codecs and partial-write offsets belong to the adapter.
pub trait ClientTransportPort: Send {
    /// Pure preflight before the logical owner mutates or performs I/O.
    fn validate_time(&self, now: Instant) -> Result<(), ClientTransportTimeError>;
    /// Reads at most the per-poll budget. EOF retains complete buffered frames;
    /// other errors are terminal and do not imply successful message delivery.
    fn read_available(&mut self) -> Result<ClientReadStatus, ClientTransportError>;
    fn front_message(&mut self) -> Result<Option<&ServerMessage>, ClientTransportError>;
    fn consume_message(&mut self);
    /// Buffer-only admission: no writes or delivery occur here. Success means
    /// capacity acceptance, not a completed write or server response. Only
    /// `flush` advances outbound delivery and reports frame completion.
    fn try_queue(&mut self, message: &ClientMessage) -> Result<ClientFrameToken, ClientQueueError>;
    /// Attempts pending writes until blocked. Supply a still-pending frame token
    /// to observe completion at the supplied host sample during this flush.
    fn flush(
        &mut self,
        tracked: Option<ClientFrameToken>,
        now: Instant,
    ) -> Result<ClientFlushProgress, ClientTransportError>;
    /// Some(true) only once all bytes for this transport-local frame were sent.
    fn frame_sent(&self, _token: ClientFrameToken) -> Option<bool> {
        None
    }
    fn metrics(&self) -> ClientTransportMetrics;
    /// Metadata used by the current separate content HTTP adapter. This exposes
    /// no socket handle or buffers; local transports may return NotConnected.
    fn peer_addr(&self) -> io::Result<SocketAddr>;
    /// Immediately closes and discards pending data, matching session shutdown.
    fn close(&mut self);
}
