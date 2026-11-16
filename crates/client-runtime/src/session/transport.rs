//! Adapts bounded transport results to logical session outcomes.
use super::*;
use minimium_transport::client::{ClientReadStatus, ClientTransportError};
use std::{error::Error, fmt, io};

/// Cloneable terminal I/O cause, retained with the session's disconnect event.
#[derive(Clone, Debug)]
pub struct SessionIoFailure(Arc<io::Error>);

impl SessionIoFailure {
    pub fn kind(&self) -> io::ErrorKind {
        self.0.kind()
    }

    pub const fn code(&self) -> &'static str {
        "client_runtime.session_io_failed"
    }
}

impl From<io::Error> for SessionIoFailure {
    fn from(error: io::Error) -> Self {
        Self(Arc::new(error))
    }
}

impl fmt::Display for SessionIoFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Error for SessionIoFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.0.as_ref())
    }
}

impl PartialEq for SessionIoFailure {
    fn eq(&self, other: &Self) -> bool {
        self.kind() == other.kind()
    }
}

impl Eq for SessionIoFailure {}

pub(super) fn queue_error(error: ClientQueueError) -> QueueError {
    match error {
        ClientQueueError::Closed => QueueError::Disconnected,
        ClientQueueError::BufferFull => QueueError::BufferFull,
        ClientQueueError::Protocol(error) => QueueError::Protocol(error),
        ClientQueueError::TokenExhausted => QueueError::TransportIdentifierExhausted,
    }
}

fn disconnect_reason(error: ClientTransportError) -> DisconnectReason {
    match error {
        ClientTransportError::Time(error) => DisconnectReason::Time(error.into()),
        ClientTransportError::Io(error) => DisconnectReason::Io(error.into()),
        ClientTransportError::Protocol(ProtocolError::UnsupportedEnvironment) => {
            DisconnectReason::EnvironmentMismatch
        }
        ClientTransportError::Protocol(error) => DisconnectReason::Protocol(error),
        ClientTransportError::BufferLimit => DisconnectReason::BufferLimit,
    }
}

impl ClientSession {
    pub(super) fn queue_transport(
        &mut self,
        message: &ClientMessage,
    ) -> Result<ClientFrameToken, ClientQueueError> {
        self.transport
            .as_mut()
            .ok_or(ClientQueueError::Closed)?
            .try_queue(message)
    }

    pub(super) fn append_encoded(&mut self, message: &ClientMessage) -> Result<(), ProtocolError> {
        self.queue_transport(message).map(|_| ()).map_err(|error| {
            if let ClientQueueError::Protocol(error) = error {
                error
            } else {
                ProtocolError::PayloadTooLarge
            }
        })
    }

    pub(super) fn front_message(&mut self) -> Result<Option<ServerMessage>, DisconnectReason> {
        match self.transport.as_mut() {
            Some(transport) => transport
                .front_message()
                .map(|message| message.cloned())
                .map_err(disconnect_reason),
            None => Ok(None),
        }
    }

    pub(super) fn consume_message(&mut self) {
        if let Some(transport) = self.transport.as_mut() {
            transport.consume_message();
        }
    }

    pub(super) fn flush_send(&mut self, events: &mut Vec<SessionEvent>) {
        let Some(transport) = self.transport.as_mut() else {
            return;
        };
        let result = transport.flush(self.pending_ping_frame, self.now.as_instant());
        let metrics = transport.metrics();
        self.metrics.bytes_sent = metrics.bytes_sent;
        self.metrics.bytes_received = metrics.bytes_received;
        match result {
            Ok(progress) => {
                if let Some(sent_at) = progress.tracked_frame_sent_at {
                    self.pending_ping_frame = None;
                    self.pending_ping_at = Some(ClientTime::new(sent_at));
                }
            }
            Err(error) => self.disconnect(disconnect_reason(error), events),
        }
    }

    pub(super) fn read_available(&mut self, events: &mut Vec<SessionEvent>) {
        let Some(transport) = self.transport.as_mut() else {
            return;
        };
        let result = transport.read_available();
        let metrics = transport.metrics();
        self.metrics.bytes_sent = metrics.bytes_sent;
        self.metrics.bytes_received = metrics.bytes_received;
        match result {
            Ok(ClientReadStatus::Open) => {}
            Ok(ClientReadStatus::PeerClosed) => {
                self.decode_available(events);
                if self.state != SessionState::Disconnected {
                    self.disconnect(DisconnectReason::PeerClosed, events);
                }
            }
            Err(error) => self.disconnect(disconnect_reason(error), events),
        }
    }
}
