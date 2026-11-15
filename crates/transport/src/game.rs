use std::{
    collections::{BTreeMap, VecDeque},
    fmt,
    io::{self, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use crate::QueueError;

pub(crate) enum GamePollError {
    Io(io::Error),
    IdentifierExhausted,
}

const MAX_ACCEPTS_PER_POLL: usize = 64;
const MAX_MESSAGES_PER_POLL: usize = 256;
const MAX_READ_BYTES_PER_CLIENT_PER_POLL: usize = 1024;
const MAX_WRITE_BYTES_PER_POLL: usize = 256 * 1024;
const MAX_WRITE_BYTES_PER_CONNECTION: usize = 16 * 1024;
const MAX_QUEUED_OUTPUT_BYTES: usize = 64 * 1024;
const MAX_QUEUED_OUTPUT_FRAMES: usize = 256;

pub(crate) struct GameState {
    pub(crate) listener: Option<TcpListener>,
    pub(crate) connections: BTreeMap<ConnectionId, GameSocket>,
    pub(crate) next_connection_id: Option<u64>,
    pub(crate) max_connections: usize,
    pub(crate) totals: GameConnectionStats,
    pub(crate) max_input_bytes: usize,
    pub(crate) max_output_bytes: usize,
    pub(crate) first_write_connection: usize,
    pub(crate) first_read_connection: usize,
    pub(crate) closed: bool,
}

pub(crate) struct GameSocket {
    pub(crate) stream: TcpStream,
    pub(crate) input: Vec<u8>,
    pub(crate) output: Vec<u8>,
    pub(crate) output_offset: usize,
    pub(crate) output_frames: VecDeque<(usize, Instant)>,
    pub(crate) close_after_flush: Option<Instant>,
    pub(crate) close_reason: Option<GameDisconnectReason>,
    pub(crate) last_message_at: Instant,
    pub(crate) peer_eof: bool,
    pub(crate) write_failed: bool,
    pub(crate) stats: GameConnectionStats,
    pub(crate) encoding_time: Duration,
}

#[derive(Clone)]
pub struct TransportSender(Arc<Mutex<GameState>>);

pub trait GameOutputPort: Send + Sync {
    fn queue_game(&self, output: GameOutput) -> Result<QueuedFrame, QueueError>;
    fn pending_output_bytes(&self, id: ConnectionId) -> Option<usize>;
    fn oldest_output_age(&self, id: ConnectionId) -> Option<Duration>;
    fn stats(&self, id: ConnectionId) -> Option<GameConnectionStats>;
    fn close(&self, id: ConnectionId);
}

#[derive(Clone, Copy, Debug)]
pub struct QueuedFrame {
    pub encoded_bytes: usize,
    pub encoding_time: Duration,
}

impl TransportSender {
    pub(crate) fn from_state(state: Arc<Mutex<GameState>>) -> Self {
        Self(state)
    }

    pub fn queue_game(&self, output: GameOutput) -> Result<QueuedFrame, QueueError> {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.closed {
            return Err(QueueError::Closed);
        }
        let encoding_time = {
            let socket = state
                .connections
                .get_mut(&output.connection_id)
                .ok_or(QueueError::UnknownConnection)?;
            if socket.close_after_flush.is_some() {
                return Err(QueueError::Closed);
            }
            if socket.output_frames.len() >= MAX_QUEUED_OUTPUT_FRAMES {
                return Err(QueueError::Backpressure);
            }
            let started = Instant::now();
            let frame = minimium_protocol::encode_server(&output.message)
                .map_err(|_| QueueError::Encoding)?;
            let encoding_time = started.elapsed();
            if frame.len() > output.max_frame_bytes {
                return Err(QueueError::FrameExceedsBudget);
            }
            let pending = socket.output.len().saturating_sub(socket.output_offset);
            if pending
                .saturating_add(frame.len())
                .saturating_add(output.reserved_bytes)
                > MAX_QUEUED_OUTPUT_BYTES
            {
                return Err(QueueError::Backpressure);
            }
            if pending.saturating_add(output.minimum_headroom_bytes) > MAX_QUEUED_OUTPUT_BYTES {
                return Err(QueueError::Backpressure);
            }
            poll::compact_output(socket);
            socket.output.extend_from_slice(&frame);
            socket
                .output_frames
                .push_back((frame.len(), Instant::now()));
            socket.stats.frames_out = socket.stats.frames_out.saturating_add(1);
            socket.encoding_time = socket.encoding_time.saturating_add(encoding_time);
            socket.close_after_flush = output.close_after_flush;
            socket.close_reason = output.close_reason;
            QueuedFrame {
                encoded_bytes: frame.len(),
                encoding_time,
            }
        };
        state.totals.frames_out = state.totals.frames_out.saturating_add(1);
        Ok(encoding_time)
    }

    pub fn pending_output_bytes(&self, id: ConnectionId) -> Option<usize> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .connections
            .get(&id)
            .map(|socket| socket.output.len().saturating_sub(socket.output_offset))
    }

    pub fn oldest_output_age(&self, id: ConnectionId) -> Option<Duration> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .connections
            .get(&id)?
            .output_frames
            .front()
            .map(|(_, at)| at.elapsed())
    }

    pub fn stats(&self, id: ConnectionId) -> Option<GameConnectionStats> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .connections
            .get(&id)
            .map(|socket| socket.stats)
    }

    pub fn encoding_time(&self, id: ConnectionId) -> Option<Duration> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .connections
            .get(&id)
            .map(|socket| socket.encoding_time)
    }

    pub fn close(&self, id: ConnectionId) {
        if let Some(socket) = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .connections
            .get_mut(&id)
        {
            socket.close_after_flush = Some(Instant::now());
        }
    }
}

impl GameOutputPort for TransportSender {
    fn queue_game(&self, output: GameOutput) -> Result<QueuedFrame, QueueError> {
        TransportSender::queue_game(self, output)
    }
    fn pending_output_bytes(&self, id: ConnectionId) -> Option<usize> {
        TransportSender::pending_output_bytes(self, id)
    }
    fn oldest_output_age(&self, id: ConnectionId) -> Option<Duration> {
        TransportSender::oldest_output_age(self, id)
    }
    fn stats(&self, id: ConnectionId) -> Option<GameConnectionStats> {
        TransportSender::stats(self, id)
    }
    fn close(&self, id: ConnectionId) {
        TransportSender::close(self, id);
    }
}

mod poll;
pub(crate) use poll::{flush_game, new_game_state, poll_game};

/// Stable, non-reused identity assigned by the host transport to a game socket.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ConnectionId(u64);

impl ConnectionId {
    /// Creates an ID for an externally hosted transport; zero is reserved.
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

impl fmt::Display for ConnectionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Typed message the runtime asks the adapter to frame and enqueue.
pub struct GameOutput {
    pub connection_id: ConnectionId,
    pub message: minimium_protocol::ServerMessage,
    pub close_after_flush: Option<Instant>,
    pub close_reason: Option<GameDisconnectReason>,
    pub reserved_bytes: usize,
    pub minimum_headroom_bytes: usize,
    pub max_frame_bytes: usize,
}

/// Typed game socket input produced by the transport poller.
pub enum GameTransportEvent {
    Connected {
        connection_id: ConnectionId,
        peer: SocketAddr,
        at: Instant,
    },
    Message {
        connection_id: ConnectionId,
        message: minimium_protocol::ClientMessage,
        at: Instant,
    },
    Closing {
        connection_id: ConnectionId,
        reason: GameDisconnectReason,
        at: Instant,
    },
    Disconnected {
        connection_id: ConnectionId,
        reason: GameDisconnectReason,
        stats: GameConnectionStats,
    },
    SocketSetupFailed {
        operation: SocketSetupOperation,
        error_kind: io::ErrorKind,
    },
}

#[derive(Clone, Copy, Debug)]
pub enum SocketSetupOperation {
    Nonblocking,
    Nodelay,
}

#[derive(Clone, Copy, Debug)]
pub enum GameDisconnectReason {
    PeerEof,
    HandshakeTimeout,
    IdleTimeout,
    InvalidFrame,
    ReadError,
    WriteError,
    Backpressure,
    Closed,
    Rejected,
    Quit,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GameConnectionStats {
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub frames_in: u64,
    pub frames_out: u64,
}
