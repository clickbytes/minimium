//! Shared, nonblocking client-side protocol session for interactive clients.
//!
//! [`ClientSession`] owns the socket and bounded wire buffers, but deliberately
//! does not own a thread, event loop, or process. Call [`ClientSession::poll`]
//! from the host's loop and pass a reusable event vector to avoid allocating an
//! event collection on each frame. Events are appended; clear the vector before
//! polling if it should contain only the latest poll's events.

use minimium_core::{
    environment::EnvironmentDescriptor, identity::PlayerProfileId, time::ClockSample,
    world::TransitionId,
};
use minimium_protocol::{
    decode_server, encode_client, ClientMessage, ClientWorldInput, ProtocolError, ServerMessage,
    WorldContext, MAX_FRAME_BYTES,
};
use std::{
    collections::HashMap,
    io::{self, Read, Write},
    net::TcpStream,
    time::{Duration, Instant},
};

mod environment;
mod receive;

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
    GeneratorMismatch,
    EarthPackage(String),
    EnvironmentMismatch,
    Io(io::ErrorKind),
    BufferLimit,
}

/// Typed output from a session poll. Every valid server message is delivered
/// as `Message`; a matching Pong also produces `PingRoundTrip`.
#[derive(Clone, Debug, PartialEq)]
// Session events are queued by value and consumed immediately after polling.
#[allow(clippy::large_enum_variant)]
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
    StaleWorldContext,
    BufferFull,
    Protocol(ProtocolError),
}

/// A single nonblocking TCP protocol session.
pub struct ClientSession {
    stream: Option<TcpStream>,
    state: SessionState,
    expected_map_fingerprint: u64,
    world_context: Option<WorldContext>,
    world_fingerprints: HashMap<minimium_core::world::WorldInstanceId, u64>,
    pending_transition: Option<(TransitionId, WorldContext)>,
    ready_transition: Option<(TransitionId, WorldContext)>,
    clock_sample: Option<ClockSample>,
    environment_descriptor: Option<EnvironmentDescriptor>,
    pending_environment: Option<(
        TransitionId,
        WorldContext,
        ClockSample,
        EnvironmentDescriptor,
    )>,
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
            world_context: None,
            world_fingerprints: HashMap::from([(
                minimium_core::world::WorldInstanceId::new(1)
                    .expect("primary world identifier is nonzero"),
                expected_map_fingerprint,
            )]),
            pending_transition: None,
            ready_transition: None,
            clock_sample: None,
            environment_descriptor: None,
            pending_environment: None,
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

    #[allow(dead_code)] // E004-05 consumes the estimate input through this accessor.
    pub fn clock_sample(&self) -> Option<ClockSample> {
        self.clock_sample
    }

    #[allow(dead_code)] // E004-05 consumes this session identity in its clock estimator.
    pub fn environment_descriptor(&self) -> Option<EnvironmentDescriptor> {
        self.environment_descriptor
    }

    pub(crate) fn set_world_fingerprints(
        &mut self,
        fingerprints: HashMap<minimium_core::world::WorldInstanceId, u64>,
    ) {
        self.world_fingerprints = fingerprints;
    }

    pub(crate) fn cancel_pending_transition(&mut self) -> bool {
        let Some((transition_id, _)) = self.pending_transition else {
            return false;
        };
        self.queue(&ClientMessage::CancelTransition(transition_id))
            .is_ok()
    }

    pub(crate) fn queue_transition_ready(
        &mut self,
        transition_id: TransitionId,
        target_context: WorldContext,
    ) -> Result<(), QueueError> {
        let transition = (transition_id, target_context);
        if self.pending_transition != Some(transition) {
            return Err(QueueError::StaleWorldContext);
        }
        if self.ready_transition == Some(transition) {
            return Ok(());
        }
        self.queue(&ClientMessage::TransitionReady {
            transition_id,
            target_context,
        })?;
        self.ready_transition = Some(transition);
        Ok(())
    }

    /// Close the current socket and discard all buffered data.
    pub fn close(&mut self) {
        self.stream = None;
        self.state = SessionState::Disconnected;
        self.pending_ping_at = None;
        self.pending_ping_end = None;
        self.world_context = None;
        self.pending_transition = None;
        self.ready_transition = None;
        self.clock_sample = None;
        self.environment_descriptor = None;
        self.pending_environment = None;
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
        let contextual = match message {
            ClientMessage::SetName(name) => Some(ClientMessage::WorldInput {
                context: self.world_context.ok_or(QueueError::AwaitingWelcome)?,
                input: ClientWorldInput::SetName(name.clone()),
            }),
            ClientMessage::Move { direction, jump } => Some(ClientMessage::WorldInput {
                context: self.world_context.ok_or(QueueError::AwaitingWelcome)?,
                input: ClientWorldInput::Move {
                    direction: *direction,
                    jump: *jump,
                },
            }),
            ClientMessage::Fire { direction } => Some(ClientMessage::WorldInput {
                context: self.world_context.ok_or(QueueError::AwaitingWelcome)?,
                input: ClientWorldInput::Fire {
                    direction: *direction,
                },
            }),
            ClientMessage::MapTeleport { direction } => Some(ClientMessage::WorldInput {
                context: self.world_context.ok_or(QueueError::AwaitingWelcome)?,
                input: ClientWorldInput::MapTeleport {
                    direction: *direction,
                },
            }),
            ClientMessage::Chat(text) => Some(ClientMessage::WorldInput {
                context: self.world_context.ok_or(QueueError::AwaitingWelcome)?,
                input: ClientWorldInput::Chat(text.clone()),
            }),
            ClientMessage::UsePortal => Some(ClientMessage::WorldInput {
                context: self.world_context.ok_or(QueueError::AwaitingWelcome)?,
                input: ClientWorldInput::UsePortal,
            }),
            ClientMessage::WorldInput { context, .. } if Some(*context) != self.world_context => {
                return Err(QueueError::StaleWorldContext);
            }
            _ => None,
        };
        let frame =
            encode_client(contextual.as_ref().unwrap_or(message)).map_err(QueueError::Protocol)?;
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
}

fn manifest_matches_generator(manifest: &minimium_core::world::WorldManifest) -> bool {
    manifest.generator_digest()
        == minimium_procgen::generator_digest(manifest.topology(), manifest.generation())
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimium_core::world::{
        GeneratorDigest, MembershipEpoch, WorldContentRevision, WorldInstanceId,
    };

    fn context(epoch: u64) -> WorldContext {
        WorldContext::new(
            WorldInstanceId::new(1).unwrap(),
            WorldContentRevision::new(1).unwrap(),
            MembershipEpoch::new(epoch).unwrap(),
        )
    }

    fn connected_session(context: WorldContext) -> ClientSession {
        let now = Instant::now();
        ClientSession {
            stream: None,
            state: SessionState::Connected,
            expected_map_fingerprint: 1,
            world_context: Some(context),
            world_fingerprints: HashMap::new(),
            pending_transition: None,
            ready_transition: None,
            clock_sample: None,
            environment_descriptor: None,
            pending_environment: None,
            state_since: now,
            last_heartbeat_at: now,
            pending_ping_at: None,
            pending_ping_end: None,
            receive_buffer: Vec::new(),
            send_buffer: Vec::new(),
            send_offset: 0,
            metrics: SessionMetrics::default(),
        }
    }

    #[test]
    fn world_inputs_from_a_previous_membership_epoch_are_rejected() {
        let current = context(8);
        let old = context(7);
        let mut session = connected_session(current);
        let input = ClientMessage::WorldInput {
            context: old,
            input: ClientWorldInput::UsePortal,
        };
        assert_eq!(session.queue(&input), Err(QueueError::StaleWorldContext));

        let current_input = ClientMessage::WorldInput {
            context: current,
            input: ClientWorldInput::UsePortal,
        };
        assert_eq!(session.queue(&current_input), Ok(()));
    }

    #[test]
    fn client_rejects_a_manifest_with_an_unknown_generator_digest() {
        let generated = minimium_procgen::generate_demo_worlds().unwrap();
        let manifest = generated[0].manifest;
        assert!(manifest_matches_generator(&manifest));
        let mismatched = minimium_core::world::WorldManifest::new(
            manifest.definition_id(),
            manifest.topology(),
            manifest.generation(),
            manifest.content_revision(),
            GeneratorDigest::new([0xff; 32]),
        );
        assert!(!manifest_matches_generator(&mismatched));
    }
}
