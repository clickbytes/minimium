//! Host-owned nonblocking game and administrative TCP transport.
mod admin;
pub mod client;
mod game;
mod host;

use std::{fmt, io};

pub use admin::{AdminInput, AdminRequestId, AdminResponse, AdminTransportEvent};
pub use game::{
    ConnectionId, GameConnectionStats, GameDisconnectReason, GameOutput, GameOutputPort,
    GameTransportEvent, QueuedFrame, SocketSetupOperation, TransportSender,
};
pub use host::{
    HostTransport, TransportConfig, TransportEndpoint, TransportEvent, TransportMetrics,
    TransportPort,
};

/// Failure to queue a typed response on its existing connection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueueError {
    UnknownConnection,
    UnknownAdminRequest,
    AdminReplyTooLarge,
    Backpressure,
    Closed,
    Encoding,
    FrameExceedsBudget,
}

impl fmt::Display for QueueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for QueueError {}

/// One bounded transport operation failed. The owning process may log the I/O cause.
#[derive(Debug)]
pub enum TransportError {
    Io(io::Error),
    IdentifierExhausted,
}

impl fmt::Display for TransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "transport I/O: {error}"),
            Self::IdentifierExhausted => formatter.write_str("transport identifiers exhausted"),
        }
    }
}

impl std::error::Error for TransportError {}
