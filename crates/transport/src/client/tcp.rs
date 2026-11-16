use super::{
    ClientFlushProgress, ClientFrameToken, ClientQueueError, ClientReadStatus,
    ClientTransportError, ClientTransportMetrics, ClientTransportPort, ClientTransportTimeError,
    MAX_CLIENT_BUFFER_BYTES, MAX_CLIENT_READ_BYTES_PER_POLL,
};
use minimium_protocol::{
    decode_server, encode_client, ClientMessage, ServerMessage, MAX_FRAME_BYTES,
};
use std::{
    io::{self, Read, Write},
    net::{SocketAddr, TcpStream},
    time::Instant,
};

/// The single TCP/framing implementation used by desktop and terminal clients.
pub struct TcpClientTransport {
    stream: Option<TcpStream>,
    receive_buffer: Vec<u8>,
    front: Option<(ServerMessage, usize)>,
    send_buffer: Vec<u8>,
    send_offset: usize,
    accepted_bytes: u64,
    completed_bytes: u64,
    peer_closed: bool,
    metrics: ClientTransportMetrics,
    last_flush_at: Option<Instant>,
}

impl TcpClientTransport {
    pub fn new(stream: TcpStream) -> io::Result<Self> {
        stream.set_nonblocking(true)?;
        stream.set_nodelay(true)?;
        Ok(Self {
            stream: Some(stream),
            receive_buffer: Vec::with_capacity(MAX_CLIENT_BUFFER_BYTES),
            front: None,
            send_buffer: Vec::with_capacity(MAX_CLIENT_BUFFER_BYTES),
            send_offset: 0,
            accepted_bytes: 0,
            completed_bytes: 0,
            peer_closed: false,
            metrics: ClientTransportMetrics::default(),
            last_flush_at: None,
        })
    }
}

impl ClientTransportPort for TcpClientTransport {
    fn validate_time(&self, now: Instant) -> Result<(), ClientTransportTimeError> {
        if self.last_flush_at.is_some_and(|previous| now < previous) {
            return Err(ClientTransportTimeError::Backward);
        }
        now.checked_add(std::time::Duration::from_secs(300))
            .ok_or(ClientTransportTimeError::Overflow)?;
        Ok(())
    }
    fn read_available(&mut self) -> Result<ClientReadStatus, ClientTransportError> {
        if self.peer_closed {
            return Ok(ClientReadStatus::PeerClosed);
        }
        let Some(stream) = self.stream.as_mut() else {
            return Ok(ClientReadStatus::PeerClosed);
        };
        let mut chunk = [0u8; 4096];
        let mut total_read = 0;
        loop {
            let available = (MAX_CLIENT_READ_BYTES_PER_POLL - total_read).min(chunk.len());
            if available == 0 {
                break;
            }
            match stream.read(&mut chunk[..available]) {
                Ok(0) => {
                    self.peer_closed = true;
                    return Ok(ClientReadStatus::PeerClosed);
                }
                Ok(count) => {
                    total_read += count;
                    self.metrics.bytes_received =
                        self.metrics.bytes_received.saturating_add(count as u64);
                    if self.receive_buffer.len().saturating_add(count) > MAX_CLIENT_BUFFER_BYTES {
                        return Err(ClientTransportError::BufferLimit);
                    }
                    self.receive_buffer.extend_from_slice(&chunk[..count]);
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(ClientTransportError::Io(error)),
            }
        }
        Ok(ClientReadStatus::Open)
    }

    fn front_message(&mut self) -> Result<Option<&ServerMessage>, ClientTransportError> {
        if self.front.is_none() {
            match decode_server(&self.receive_buffer).map_err(ClientTransportError::Protocol)? {
                Some(decoded) => self.front = Some(decoded),
                None if self.receive_buffer.len() > MAX_FRAME_BYTES => {
                    return Err(ClientTransportError::BufferLimit);
                }
                None => {}
            }
        }
        Ok(self.front.as_ref().map(|(message, _)| message))
    }

    fn consume_message(&mut self) {
        if let Some((_, frame_bytes)) = self.front.take() {
            self.receive_buffer.drain(..frame_bytes);
        }
    }

    fn try_queue(&mut self, message: &ClientMessage) -> Result<ClientFrameToken, ClientQueueError> {
        if self.stream.is_none() {
            return Err(ClientQueueError::Closed);
        }
        let frame = encode_client(message).map_err(ClientQueueError::Protocol)?;
        if self.send_offset > 0 {
            self.send_buffer.drain(..self.send_offset);
            self.send_offset = 0;
        }
        if self.send_buffer.len().saturating_add(frame.len()) > MAX_CLIENT_BUFFER_BYTES {
            return Err(ClientQueueError::BufferFull);
        }
        let end = self
            .accepted_bytes
            .checked_add(frame.len() as u64)
            .ok_or(ClientQueueError::TokenExhausted)?;
        self.send_buffer.extend_from_slice(&frame);
        self.accepted_bytes = end;
        // Encoded frames are nonempty, so the checked cumulative end is nonzero.
        Ok(ClientFrameToken(end))
    }

    fn flush(
        &mut self,
        tracked: Option<ClientFrameToken>,
        now: Instant,
    ) -> Result<ClientFlushProgress, ClientTransportError> {
        self.validate_time(now)
            .map_err(ClientTransportError::Time)?;
        self.last_flush_at = Some(now);
        let mut progress = ClientFlushProgress::default();
        while self.send_offset < self.send_buffer.len() {
            let result = match self.stream.as_mut() {
                Some(stream) => stream.write(&self.send_buffer[self.send_offset..]),
                None => return Ok(progress),
            };
            match result {
                Ok(0) => {
                    return Err(ClientTransportError::Io(io::Error::from(
                        io::ErrorKind::WriteZero,
                    )));
                }
                Ok(count) => {
                    let previous_completed = self.completed_bytes;
                    self.send_offset += count;
                    self.completed_bytes =
                        self.accepted_bytes - (self.send_buffer.len() - self.send_offset) as u64;
                    self.metrics.bytes_sent = self.metrics.bytes_sent.saturating_add(count as u64);
                    if tracked.is_some_and(|token| {
                        previous_completed < token.get() && self.completed_bytes >= token.get()
                    }) {
                        progress.tracked_frame_sent_at = Some(now);
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(ClientTransportError::Io(error)),
            }
        }
        if self.send_offset == self.send_buffer.len() {
            self.send_buffer.clear();
            self.send_offset = 0;
        }
        Ok(progress)
    }

    fn metrics(&self) -> ClientTransportMetrics {
        self.metrics
    }

    fn peer_addr(&self) -> io::Result<SocketAddr> {
        self.stream
            .as_ref()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotConnected))?
            .peer_addr()
    }

    fn close(&mut self) {
        self.stream = None;
        self.receive_buffer.clear();
        self.front = None;
        self.send_buffer.clear();
        self.send_offset = 0;
        self.peer_closed = true;
    }
}
