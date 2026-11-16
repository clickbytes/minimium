//! Shared, nonblocking client-side protocol session for interactive clients.
//!
//! [`ClientSession`] owns logical protocol state and an injected message transport,
//! but does not own an event loop or process. Call [`ClientSession::poll`]
//! from the host's loop and pass a reusable event vector to avoid allocating an
//! event collection on each frame. Events are appended; clear the vector before
//! polling if it should contain only the latest poll's events.

use crate::world_content::{
    ContentResolutionTicket, ResolvedWorldContent, WorldContentRequest, WorldContentResolver,
};
use minimium_environment::{time::ClockSample, EnvironmentDescriptor};
use minimium_protocol::{ClientMessage, ProtocolError, ServerMessage, WorldContext};
use minimium_transport::client::{ClientFrameToken, ClientQueueError, ClientTransportPort};
use minimium_world::entity::{ComponentTypeId, SchemaVersion};
use minimium_world::world::TransitionId;
use std::{
    collections::HashMap,
    sync::Arc,
    thread::JoinHandle,
    time::{Duration, Instant},
};

mod failure;
pub use failure::SessionFailure;

mod clock;
mod content;
mod content_access;
mod environment;
mod excavation;
mod impacts;
mod receive;
mod replication;
mod transfer;
mod transport;
pub use transport::SessionIoFailure;

/// Maximum retained receive or pending send data.
pub const MAX_SESSION_BUFFER_BYTES: usize = minimium_transport::client::MAX_CLIENT_BUFFER_BYTES;
/// Maximum amount read in a single poll, keeping UI loops responsive.
pub const MAX_SESSION_READ_BYTES_PER_POLL: usize =
    minimium_transport::client::MAX_CLIENT_READ_BYTES_PER_POLL;
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
    Io(SessionIoFailure),
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

/// Identifies one prepared transfer at both the protocol and destination context.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TransferTicket {
    transition_id: TransitionId,
    context: WorldContext,
}

impl TransferTicket {
    pub const fn new(transition_id: TransitionId, context: WorldContext) -> Self {
        Self {
            transition_id,
            context,
        }
    }

    pub const fn transition_id(self) -> TransitionId {
        self.transition_id
    }
    pub const fn context(self) -> WorldContext {
        self.context
    }
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
    TransportIdentifierExhausted,
    Protocol(ProtocolError),
}

/// Logical session over a host-supplied bounded nonblocking message transport.
pub struct ClientSession {
    pub(crate) impacts: crate::impacts::ImpactReplica,
    pub(crate) excavation: crate::excavation::ExcavationReplica,
    tools: excavation::ToolClientState,
    diagnostic_logger: Option<minimium_diagnostics::DomainLogger>,
    content: content::ContentPreparation,
    transport: Option<Box<dyn ClientTransportPort>>,
    state: SessionState,
    world_context: Option<WorldContext>,
    server_tick: u64,
    next_input_seq: u64,
    next_target_tick: u64,
    session_id: u64,
    world_content_resolver: Option<Arc<dyn WorldContentResolver>>,
    world_content_worker: Option<JoinHandle<receive::PreparedWorldContent>>,
    world_content_worker_ticket: Option<ContentResolutionTicket>,
    world_content_request: Option<ContentResolutionTicket>,
    next_world_content_request_id: u64,
    world_content_cancel: crate::ContentCancellation,
    world_content_started: Option<Instant>,
    deferred_world_message: Option<(ServerMessage, u64)>,
    active_world_content: Option<ActiveWorldContent>,
    pending_world_content: Option<PendingWorldContent>,
    replica_schemas: HashMap<ComponentTypeId, SchemaVersion>,
    replica_assembly: Option<crate::replicas::ReplicaAssembly>,
    replicas: crate::replicas::ReplicatedEntities,
    pending_transition: Option<(TransitionId, WorldContext)>,
    ready_transition: Option<(TransitionId, WorldContext)>,
    cancel_requested_transition: Option<TransitionId>,
    host_ready_transition: Option<(TransitionId, WorldContext)>,
    pending_prepare_identity: Option<transfer::PrepareIdentity>,
    highest_terminated_transition_id: Option<TransitionId>,
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
    pending_ping_frame: Option<ClientFrameToken>,
    metrics: SessionMetrics,
    content_cache_dir: Option<std::path::PathBuf>,
    content_cache_limit_bytes: u64,
}

struct ActiveWorldContent {
    context: WorldContext,
    content: Arc<ResolvedWorldContent>,
    water_catalog: Option<Arc<minimium_procgen::WaterCatalog>>,
}

struct PendingWorldContent {
    context: WorldContext,
    transition_id: TransitionId,
    ticket: ContentResolutionTicket,
    content: Arc<ResolvedWorldContent>,
    water_catalog: Option<Arc<minimium_procgen::WaterCatalog>>,
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

    /// Starts a fresh session with an already configured transport and queues
    /// its handshake. Queue acceptance is distinct from writing or admission.
    pub fn new(
        transport: Box<dyn ClientTransportPort>,
        authentication: crate::authentication::ClientAuthentication,
    ) -> Result<Self, ClientQueueError> {
        Self::with_presentation(transport, authentication, None)
    }

    pub fn with_presentation(
        transport: Box<dyn ClientTransportPort>,
        authentication: crate::authentication::ClientAuthentication,
        capability: Option<bool>,
    ) -> Result<Self, ClientQueueError> {
        let now = Instant::now();
        let mut content = content::ContentPreparation::default();
        content.capability = capability;
        let mut session = Self {
            diagnostic_logger: None,
            content,
            transport: Some(transport),
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
            active_world_content: None,
            pending_world_content: None,
            impacts: Default::default(),
            excavation: Default::default(),
            tools: Default::default(),
            replica_schemas: HashMap::new(),
            replica_assembly: None,
            replicas: crate::replicas::ReplicatedEntities::default(),
            pending_transition: None,
            ready_transition: None,
            cancel_requested_transition: None,
            host_ready_transition: None,
            pending_prepare_identity: None,
            highest_terminated_transition_id: None,
            clock_sample: None,
            environment_descriptor: None,
            pending_environment: None,
            clock: clock::ClientClock::new(now, None),
            state_since: now,
            last_heartbeat_at: now,
            pending_ping_at: None,
            pending_ping_frame: None,
            metrics: SessionMetrics::default(),
            content_cache_dir: None,
            content_cache_limit_bytes: minimium_asset_io::DEFAULT_CACHE_LIMIT_BYTES,
        };
        let handshake = session
            .queue_transport(&ClientMessage::Presentation(capability))
            .and_then(|_| {
                session.queue_transport(&ClientMessage::Authenticate {
                    provider_id: authentication.provider_id().to_owned(),
                    credentials: authentication.credentials().to_vec(),
                })
            });
        if let Err(error) = handshake {
            session.close();
            return Err(error);
        }
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

    /// Close the current transport and discard all buffered data.
    pub fn close(&mut self) {
        self.content = Default::default();
        if let Some(mut transport) = self.transport.take() {
            transport.close();
        }
        self.state = SessionState::Disconnected;
        self.pending_ping_at = None;
        self.pending_ping_frame = None;
        self.world_context = None;
        self.active_world_content = None;
        self.pending_world_content = None;
        self.world_content_cancel.cancel();
        self.world_content_worker = None;
        self.world_content_worker_ticket = None;
        self.world_content_request = None;
        self.world_content_started = None;
        self.deferred_world_message = None;
        self.impacts = Default::default();
        self.excavation = Default::default();
        self.tools = Default::default();
        self.clear_transfer_lifecycle();
        self.clock_sample = None;
        self.environment_descriptor = None;
        self.pending_environment = None;
        self.clock.reset();
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
        let frame = self
            .queue_transport(message)
            .map_err(transport::queue_error)?;
        if matches!(message, ClientMessage::Ping)
            && self.pending_ping_at.is_none()
            && self.pending_ping_frame.is_none()
        {
            self.pending_ping_frame = Some(frame);
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

impl Drop for ClientSession {
    fn drop(&mut self) {
        if let Some(transport) = self.transport.as_mut() {
            transport.close();
        }
    }
}

fn manifest_matches_generator(manifest: &minimium_world::world::WorldManifest) -> bool {
    manifest.source() == minimium_world::world::WorldSource::Generated
        && minimium_procgen::validate_world_manifest(*manifest).is_ok()
}

#[cfg(test)]
#[path = "session/tests.rs"]
mod tests;
