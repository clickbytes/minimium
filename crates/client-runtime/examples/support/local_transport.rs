//! Bounded bidirectional local adapter for an embedded host, compiled only.
use minimium_protocol::{encode_client, encode_server, ClientMessage, ServerMessage};
use minimium_transport::client::{
    ClientFlushProgress, ClientFrameToken, ClientQueueError, ClientReadStatus,
    ClientTransportError, ClientTransportMetrics, ClientTransportPort, ClientTransportTimeError,
    MAX_CLIENT_BUFFER_BYTES, MAX_CLIENT_READ_BYTES_PER_POLL,
};
use std::{
    collections::VecDeque,
    io,
    net::SocketAddr,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard, TryLockError,
    },
    time::Instant,
};

#[derive(Default)]
struct PeerState {
    pending: VecDeque<(ServerMessage, usize)>,
    pending_bytes: usize,
    delivered: VecDeque<(ClientMessage, usize)>,
    delivered_bytes: usize,
}

#[derive(Default)]
struct SharedPeer {
    state: Mutex<PeerState>,
    closed: AtomicBool,
}

impl SharedPeer {
    fn try_state(&self) -> Option<MutexGuard<'_, PeerState>> {
        match self.state.try_lock() {
            Ok(state) => Some(state),
            Err(TryLockError::WouldBlock) => None,
            Err(TryLockError::Poisoned(error)) => Some(error.into_inner()),
        }
    }
}

#[derive(Clone)]
pub struct LocalPeer(Arc<SharedPeer>);

impl LocalPeer {
    pub fn try_send_server(&self, message: ServerMessage) -> Result<(), ClientQueueError> {
        if self.0.closed.load(Ordering::Acquire) {
            return Err(ClientQueueError::Closed);
        }
        let bytes = encode_server(&message)
            .map_err(ClientQueueError::Protocol)?
            .len();
        let mut state = self.0.try_state().ok_or(ClientQueueError::BufferFull)?;
        if self.0.closed.load(Ordering::Acquire) {
            return Err(ClientQueueError::Closed);
        }
        if state.pending_bytes.saturating_add(bytes) > MAX_CLIENT_BUFFER_BYTES {
            return Err(ClientQueueError::BufferFull);
        }
        state.pending_bytes += bytes;
        state.pending.push_back((message, bytes));
        Ok(())
    }

    pub fn receive_client(&self) -> Option<ClientMessage> {
        let mut state = self.0.try_state()?;
        let (message, bytes) = state.delivered.pop_front()?;
        state.delivered_bytes -= bytes;
        Some(message)
    }
}

#[derive(Default)]
pub struct LocalTransport {
    incoming: VecDeque<(ServerMessage, usize)>,
    incoming_bytes: usize,
    outgoing: VecDeque<(ClientFrameToken, ClientMessage, usize)>,
    outgoing_bytes: usize,
    accepted_bytes: u64,
    closed: bool,
    peer: Arc<SharedPeer>,
    metrics: ClientTransportMetrics,
    last_flush_at: Option<Instant>,
}

impl LocalTransport {
    pub fn peer(&self) -> LocalPeer {
        LocalPeer(self.peer.clone())
    }
}

impl Drop for LocalTransport {
    fn drop(&mut self) {
        self.close();
    }
}

impl ClientTransportPort for LocalTransport {
    fn validate_time(&self, now: Instant) -> Result<(), ClientTransportTimeError> {
        if self.last_flush_at.is_some_and(|previous| now < previous) {
            return Err(ClientTransportTimeError::Backward);
        }
        now.checked_add(std::time::Duration::from_secs(300))
            .ok_or(ClientTransportTimeError::Overflow)?;
        Ok(())
    }
    fn read_available(&mut self) -> Result<ClientReadStatus, ClientTransportError> {
        if self.closed {
            return Ok(ClientReadStatus::PeerClosed);
        }
        let Some(mut state) = self.peer.try_state() else {
            return Ok(ClientReadStatus::Open);
        };
        let mut read = 0usize;
        while let Some((_, bytes)) = state.pending.front() {
            if read.saturating_add(*bytes) > MAX_CLIENT_READ_BYTES_PER_POLL
                || self.incoming_bytes.saturating_add(*bytes) > MAX_CLIENT_BUFFER_BYTES
            {
                break;
            }
            let (message, bytes) = state.pending.pop_front().expect("front exists");
            state.pending_bytes -= bytes;
            read += bytes;
            self.incoming_bytes += bytes;
            self.incoming.push_back((message, bytes));
        }
        self.metrics.bytes_received = self.metrics.bytes_received.saturating_add(read as u64);
        Ok(ClientReadStatus::Open)
    }

    fn front_message(&mut self) -> Result<Option<&ServerMessage>, ClientTransportError> {
        Ok(self.incoming.front().map(|(message, _)| message))
    }

    fn consume_message(&mut self) {
        if let Some((_, bytes)) = self.incoming.pop_front() {
            self.incoming_bytes -= bytes;
        }
    }

    fn try_queue(&mut self, message: &ClientMessage) -> Result<ClientFrameToken, ClientQueueError> {
        if self.closed {
            return Err(ClientQueueError::Closed);
        }
        let bytes = encode_client(message)
            .map_err(ClientQueueError::Protocol)?
            .len();
        if self.outgoing_bytes.saturating_add(bytes) > MAX_CLIENT_BUFFER_BYTES {
            return Err(ClientQueueError::BufferFull);
        }
        let end = self
            .accepted_bytes
            .checked_add(bytes as u64)
            .ok_or(ClientQueueError::TokenExhausted)?;
        let token = ClientFrameToken::new(end).ok_or(ClientQueueError::TokenExhausted)?;
        self.accepted_bytes = end;
        self.outgoing_bytes += bytes;
        self.outgoing.push_back((token, message.clone(), bytes));
        Ok(token)
    }

    fn flush(
        &mut self,
        tracked: Option<ClientFrameToken>,
        now: Instant,
    ) -> Result<ClientFlushProgress, ClientTransportError> {
        self.validate_time(now)
            .map_err(ClientTransportError::Time)?;
        self.last_flush_at = Some(now);
        let mut tracked_frame_sent_at = None;
        let Some(mut state) = self.peer.try_state() else {
            return Ok(ClientFlushProgress::default());
        };
        while let Some((_, _, bytes)) = self.outgoing.front() {
            if state.delivered_bytes.saturating_add(*bytes) > MAX_CLIENT_BUFFER_BYTES {
                break;
            }
            let (token, message, bytes) = self.outgoing.pop_front().expect("front exists");
            state.delivered_bytes += bytes;
            state.delivered.push_back((message, bytes));
            self.outgoing_bytes -= bytes;
            self.metrics.bytes_sent = self.metrics.bytes_sent.saturating_add(bytes as u64);
            if tracked == Some(token) {
                tracked_frame_sent_at = Some(now);
            }
        }
        Ok(ClientFlushProgress {
            tracked_frame_sent_at,
        })
    }

    fn metrics(&self) -> ClientTransportMetrics {
        self.metrics
    }

    fn peer_addr(&self) -> io::Result<SocketAddr> {
        Err(io::Error::new(
            io::ErrorKind::AddrNotAvailable,
            "local transport has no HTTP peer",
        ))
    }

    fn close(&mut self) {
        self.closed = true;
        self.incoming.clear();
        self.outgoing.clear();
        self.incoming_bytes = 0;
        self.outgoing_bytes = 0;
        self.peer.closed.store(true, Ordering::Release);
        if let Some(mut state) = self.peer.try_state() {
            state.pending.clear();
            state.pending_bytes = 0;
        }
    }
}
