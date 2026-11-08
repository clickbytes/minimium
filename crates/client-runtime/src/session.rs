//! Shared, nonblocking client-side protocol session for interactive clients.
//!
//! [`ClientSession`] owns the socket and bounded wire buffers, but deliberately
//! does not own a thread, event loop, or process. Call [`ClientSession::poll`]
//! from the host's loop and pass a reusable event vector to avoid allocating an
//! event collection on each frame. Events are appended; clear the vector before
//! polling if it should contain only the latest poll's events.

use crate::world_content::{
    ContentResolutionTicket, ResolvedWorldContent, WorldContentRequest, WorldContentResolver,
};
use minimium_environment::{time::ClockSample, EnvironmentDescriptor};
use minimium_protocol::{
    decode_server, encode_client, ClientMessage, ProtocolError, ServerMessage, WorldContext,
    MAX_FRAME_BYTES,
};
use minimium_world::entity::{ComponentTypeId, SchemaVersion};
use minimium_world::world::TransitionId;
use std::{
    collections::HashMap,
    error::Error,
    fmt,
    io::{self, Read, Write},
    net::TcpStream,
    sync::Arc,
    thread::JoinHandle,
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct SessionFailure {
    code: &'static str,
    message: String,
    source: Option<Arc<dyn Error + Send + Sync>>,
}

impl SessionFailure {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            source: None,
        }
    }

    pub fn caused_by<E>(code: &'static str, message: impl Into<String>, source: E) -> Self
    where
        E: Error + Send + Sync + 'static,
    {
        Self {
            code,
            message: message.into(),
            source: Some(Arc::new(source)),
        }
    }

    pub fn from_asset_error(error: minimium_asset_io::AssetIoError) -> Self {
        let code = error.code();
        Self::caused_by(code, error.to_string(), error)
    }

    pub fn code(&self) -> &'static str {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for SessionFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for SessionFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn Error + 'static))
    }
}

impl PartialEq for SessionFailure {
    fn eq(&self, other: &Self) -> bool {
        self.code == other.code && self.message == other.message
    }
}

impl Eq for SessionFailure {}

mod clock;
mod content;
mod environment;
mod receive;
mod replication;

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
    WorldContent(SessionFailure),
    EarthPackage(SessionFailure),
    Materials(SessionFailure),
    EnvironmentMismatch,
    Io(io::ErrorKind),
    BufferLimit,
    Replication(crate::ReplicaError),
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
    AuthenticateReserved,
    StaleWorldContext,
    BufferFull,
    InputSequenceExhausted,
    Protocol(ProtocolError),
}

/// A single nonblocking TCP protocol session.
pub struct ClientSession {
    diagnostic_logger: Option<minimium_diagnostics::DomainLogger>,
    content: content::ContentPreparation,
    stream: Option<TcpStream>,
    state: SessionState,
    world_context: Option<WorldContext>,
    server_tick: u64,
    next_input_seq: u64,
    next_target_tick: u64,
    session_id: u64,
    world_content_resolver: Option<Arc<dyn WorldContentResolver>>,
    world_content_worker: Option<JoinHandle<Result<ResolvedWorldContent, SessionFailure>>>,
    world_content_worker_ticket: Option<ContentResolutionTicket>,
    world_content_request: Option<ContentResolutionTicket>,
    next_world_content_request_id: u64,
    world_content_cancel: crate::ContentCancellation,
    world_content_started: Option<Instant>,
    deferred_world_message: Option<(ServerMessage, u64)>,
    resolved_worlds: HashMap<minimium_world::world::WorldInstanceId, Arc<ResolvedWorldContent>>,
    replica_schemas: HashMap<ComponentTypeId, SchemaVersion>,
    replica_assembly: Option<crate::replicas::ReplicaAssembly>,
    replicas: crate::replicas::ReplicatedEntities,
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
    clock: clock::ClientClock,
    state_since: Instant,
    last_heartbeat_at: Instant,
    pending_ping_at: Option<Instant>,
    pending_ping_end: Option<usize>,
    receive_buffer: Vec<u8>,
    send_buffer: Vec<u8>,
    send_offset: usize,
    metrics: SessionMetrics,
    prepared_geography: Option<Arc<dyn minimium_procgen::PreparedGeography>>,
    content_cache_dir: Option<std::path::PathBuf>,
    content_cache_limit_bytes: u64,
}

impl ClientSession {
    fn log_failure(&self, failure: &SessionFailure) {
        if let Some(logger) = &self.diagnostic_logger {
            crate::diagnostics::emit(
                Some(logger),
                minimium_diagnostics::Level::Error,
                failure.code(),
                [crate::diagnostics::field("reason", failure.to_string())],
            );
        }
    }

    /// Starts a fresh session, sets the supplied stream nonblocking, and queues
    /// the authentication handshake. The first poll writes it.
    pub fn new(
        stream: TcpStream,
        authentication: crate::authentication::ClientAuthentication,
    ) -> io::Result<Self> {
        Self::with_presentation(stream, authentication, None)
    }

    pub fn with_presentation(
        stream: TcpStream,
        authentication: crate::authentication::ClientAuthentication,
        capability: Option<bool>,
    ) -> io::Result<Self> {
        stream.set_nonblocking(true)?;
        stream.set_nodelay(true)?;
        let now = Instant::now();
        let mut content = content::ContentPreparation::default();
        content.capability = capability;
        let mut session = Self {
            diagnostic_logger: None,
            content,
            stream: Some(stream),
            state: SessionState::AwaitingWelcome,
            world_context: None,
            server_tick: 0,
            next_input_seq: 1,
            next_target_tick: 2,
            session_id: 0,
            world_content_resolver: None,
            world_content_worker: None,
            world_content_worker_ticket: None,
            world_content_request: None,
            next_world_content_request_id: 1,
            world_content_cancel: crate::ContentCancellation::default(),
            world_content_started: None,
            deferred_world_message: None,
            resolved_worlds: HashMap::new(),
            replica_schemas: HashMap::new(),
            replica_assembly: None,
            replicas: crate::replicas::ReplicatedEntities::default(),
            pending_transition: None,
            ready_transition: None,
            clock_sample: None,
            environment_descriptor: None,
            pending_environment: None,
            clock: clock::ClientClock::new(now, None),
            state_since: now,
            last_heartbeat_at: now,
            pending_ping_at: None,
            pending_ping_end: None,
            receive_buffer: Vec::with_capacity(MAX_SESSION_BUFFER_BYTES),
            send_buffer: Vec::with_capacity(MAX_SESSION_BUFFER_BYTES),
            send_offset: 0,
            metrics: SessionMetrics::default(),
            prepared_geography: None,
            content_cache_dir: None,
            content_cache_limit_bytes: minimium_asset_io::DEFAULT_CACHE_LIMIT_BYTES,
        };
        session
            .append_encoded(&ClientMessage::Presentation(capability))
            .expect("presentation frame is bounded");
        session
            .append_encoded(&ClientMessage::Authenticate {
                provider_id: authentication.provider_id().to_owned(),
                credentials: authentication.credentials().to_vec(),
            })
            .expect("developer authentication frame is within protocol limits");
        Ok(session)
    }

    pub(crate) fn set_content_cache_dir(&mut self, cache_dir: Option<std::path::PathBuf>) {
        self.content_cache_dir = cache_dir;
    }

    pub(crate) fn set_content_cache_limit(&mut self, maximum_bytes: u64) {
        self.content_cache_limit_bytes = maximum_bytes.max(1);
    }

    pub(crate) fn set_diagnostic_logger(
        &mut self,
        logger: Option<minimium_diagnostics::DomainLogger>,
    ) {
        self.clock.set_logger(logger.clone());
        self.diagnostic_logger = logger;
    }

    pub fn state(&self) -> SessionState {
        self.state
    }

    pub fn metrics(&self) -> SessionMetrics {
        self.metrics
    }

    pub fn prepared_geography(&self) -> Option<Arc<dyn minimium_procgen::PreparedGeography>> {
        self.prepared_geography.clone()
    }

    pub fn clock_sample(&self) -> Option<ClockSample> {
        self.clock_sample
    }

    pub fn server_monotonic_ms(&self) -> Option<u64> {
        self.clock.server_monotonic_ms()
    }

    pub fn environment_descriptor(&self) -> Option<EnvironmentDescriptor> {
        self.environment_descriptor
    }

    pub fn environment_frame(&self) -> crate::environment::EnvironmentFrameInput {
        self.clock.frame()
    }

    pub(crate) fn set_world_content_resolver(
        &mut self,
        session_id: u64,
        resolver: Option<Arc<dyn WorldContentResolver>>,
    ) {
        self.session_id = session_id;
        self.world_content_resolver = resolver;
    }

    pub fn resolved_world_content(
        &self,
        instance: minimium_world::world::WorldInstanceId,
    ) -> Option<Arc<ResolvedWorldContent>> {
        self.resolved_worlds.get(&instance).cloned()
    }

    pub fn cancel_pending_transition(&mut self) -> bool {
        let Some((transition_id, _)) = self.pending_transition else {
            return false;
        };
        self.queue(&ClientMessage::CancelTransition(transition_id))
            .is_ok()
    }

    pub fn queue_transition_ready(
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
        self.content = Default::default();
        self.stream = None;
        self.state = SessionState::Disconnected;
        self.pending_ping_at = None;
        self.pending_ping_end = None;
        self.world_context = None;
        self.resolved_worlds.clear();
        self.world_content_cancel.cancel();
        self.world_content_worker = None;
        self.world_content_worker_ticket = None;
        self.world_content_request = None;
        self.world_content_started = None;
        self.deferred_world_message = None;
        self.prepared_geography = None;
        self.pending_transition = None;
        self.ready_transition = None;
        self.clock_sample = None;
        self.environment_descriptor = None;
        self.pending_environment = None;
        self.clock.reset();
        self.receive_buffer.clear();
        self.send_buffer.clear();
        self.send_offset = 0;
    }

    /// Queues an application message. Authentication is reserved for construction;
    /// all other messages are accepted only after Welcome. A full bounded send
    /// buffer is reported without disconnecting so the caller can retry.
    pub fn queue(&mut self, message: &ClientMessage) -> Result<(), QueueError> {
        match self.state {
            SessionState::Disconnected => return Err(QueueError::Disconnected),
            SessionState::AwaitingWelcome => return Err(QueueError::AwaitingWelcome),
            SessionState::Connected => {}
        }
        if matches!(message, ClientMessage::Authenticate { .. }) {
            return Err(QueueError::AuthenticateReserved);
        }
        if let ClientMessage::WorldInput { context, .. } | ClientMessage::SkyView { context, .. } =
            message
        {
            if Some(*context) != self.world_context {
                return Err(QueueError::StaleWorldContext);
            }
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

    pub fn queue_world_input(
        &mut self,
        mut input: minimium_protocol::ClientWorldInput,
    ) -> Result<Option<minimium_protocol::MovementSample>, QueueError> {
        let context = self.world_context.ok_or(QueueError::AwaitingWelcome)?;
        let mut queued_sample = None;
        let mut motion_counters = None;
        if let minimium_protocol::ClientWorldInput::Motion(sample) = &mut input {
            let input_seq = self.next_input_seq;
            let next_input_seq = input_seq
                .checked_add(1)
                .ok_or(QueueError::InputSequenceExhausted)?;
            sample.input_seq = input_seq;
            let target_tick = self
                .next_target_tick
                .max(self.server_tick.saturating_add(2).max(1));
            sample.target_tick = target_tick;
            let next_target_tick = target_tick.saturating_add(1);
            motion_counters = Some((next_input_seq, next_target_tick));
            queued_sample = Some(*sample);
        }
        self.queue(&ClientMessage::WorldInput { context, input })?;
        if let Some((next_input_seq, next_target_tick)) = motion_counters {
            self.next_input_seq = next_input_seq;
            self.next_target_tick = next_target_tick;
        }
        Ok(queued_sample)
    }
}

fn manifest_matches_generator(manifest: &minimium_world::world::WorldManifest) -> bool {
    manifest.source() == minimium_world::world::WorldSource::Generated
        && minimium_procgen::validate_world_manifest(*manifest).is_ok()
}

#[cfg(test)]
#[path = "session/tests.rs"]
mod tests;
