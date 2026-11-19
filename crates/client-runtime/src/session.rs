//! Shared, nonblocking client-side protocol session for interactive clients.
//!
//! [`ClientSession`] owns logical protocol state and an injected message transport,
//! but does not own an event loop or process. Call [`ClientSession::poll`]
//! from the host's loop and pass a reusable event vector to avoid allocating an
//! event collection on each frame. Events are appended; clear the vector before
//! polling if it should contain only the latest poll's events.

use crate::world_content::{ContentResolutionTicket, ResolvedWorldContent, WorldContentRequest};
use crate::{
    ClientTime, ClientTimeError, ContentJobHandle, ContentJobsSession, PreparedWorldContent,
};
use minimium_environment::{time::ClockSample, EnvironmentDescriptor};
use minimium_protocol::{ClientMessage, ProtocolError, ServerMessage, WorldContext};
use minimium_transport::client::{ClientFrameToken, ClientQueueError, ClientTransportPort};
use minimium_world::entity::{ComponentTypeId, SchemaVersion};
use minimium_world::world::TransitionId;
use std::{collections::HashMap, sync::Arc, time::Duration};

mod failure;
pub use failure::SessionFailure;

mod clock;
mod content;
mod content_access;
mod environment;
mod excavation;
mod impacts;
mod operation_error;
mod receive;
mod replication;
mod transfer;
mod transport;
mod world_jobs;
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
    Time(ClientTimeError),
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

mod metrics;
pub use metrics::{QueuedWorldFrame, SessionMetrics};

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
    Time(ClientTimeError),
}

/// Logical session over a host-supplied bounded nonblocking message transport.
pub struct ClientSession {
    pub(crate) impacts: crate::impacts::ImpactReplica,
    pub(crate) excavation: crate::excavation::ExcavationReplica,
    tools: excavation::ToolClientState,
    diagnostic_logger: Option<minimium_diagnostics::DomainLogger>,
    content: content::ContentPreparation,
    transport: Option<Box<dyn ClientTransportPort>>,
    actor: Option<minimium_core::ids::PlayerId>,
    state: SessionState,
    world_context: Option<WorldContext>,
    server_tick: u64,
    next_input_seq: u64,
    next_world_sequence: u64,
    content_jobs: ContentJobsSession,
    world_content_job: Option<ContentJobHandle>,
    world_content_result: Option<Result<PreparedWorldContent, SessionFailure>>,
    world_content_request: Option<WorldContentRequest>,
    next_world_content_request_id: u64,
    next_material_content_request_id: u64,
    world_content_started: Option<ClientTime>,
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
    now: ClientTime,
    state_since: ClientTime,
    last_heartbeat_at: ClientTime,
    pending_ping_at: Option<ClientTime>,
    pending_ping_frame: Option<ClientFrameToken>,
    metrics: SessionMetrics,
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
    pub(crate) fn next_movement_input_seq(&self) -> u64 {
        self.next_input_seq
    }
    pub(crate) fn next_world_request_id(&self) -> u64 {
        self.next_world_sequence
    }
    /// Host-local session identity shared by content jobs and correlated operation results.
    pub fn content_jobs_session_id(&self) -> u64 {
        self.content_jobs.session_id()
    }

    pub(crate) fn content_jobs_port(&self) -> Arc<dyn crate::ContentJobsPort> {
        self.content_jobs.port_arc()
    }

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
        content_jobs: ContentJobsSession,
        now: ClientTime,
    ) -> Result<Self, QueueError> {
        Self::with_presentation(transport, authentication, None, content_jobs, now)
    }

    pub fn with_presentation(
        transport: Box<dyn ClientTransportPort>,
        authentication: crate::authentication::ClientAuthentication,
        capability: Option<bool>,
        content_jobs: ContentJobsSession,
        now: ClientTime,
    ) -> Result<Self, QueueError> {
        now.validate_after(now).map_err(QueueError::Time)?;
        let mut content = content::ContentPreparation::default();
        content.capability = capability;
        let mut session = Self {
            diagnostic_logger: None,
            content,
            transport: Some(transport),
            actor: None,
            state: SessionState::AwaitingWelcome,
            world_context: None,
            server_tick: 0,
            next_input_seq: 1,
            next_world_sequence: 1,
            content_jobs,
            world_content_job: None,
            world_content_result: None,
            world_content_request: None,
            next_world_content_request_id: 1,
            next_material_content_request_id: 1,
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
            now,
            state_since: now,
            last_heartbeat_at: now,
            pending_ping_at: None,
            pending_ping_frame: None,
            metrics: SessionMetrics::default(),
        };
        session.validate_time(now).map_err(QueueError::Time)?;
        let handshake = session
            .queue_transport(&ClientMessage::Presentation(capability))
            .and_then(|_| {
                session.queue_transport(&ClientMessage::Authenticate {
                    provider_id: authentication.provider_id().to_owned(),
                    credentials: authentication.credentials().to_vec(),
                })
            });
        if let Err(error) = handshake {
            session.close_resources();
            return Err(transport::queue_error(error));
        }
        Ok(session)
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

    /// Pure preflight for a host operation, including the injected port.
    pub fn validate_time(&self, now: ClientTime) -> Result<(), ClientTimeError> {
        now.validate_after(self.now)?;
        self.clock.validate_time(now)?;
        if let Some(transport) = self.transport.as_ref() {
            transport
                .validate_time(now.as_instant())
                .map_err(ClientTimeError::from)?;
        }
        Ok(())
    }

    pub fn current_time(&self) -> ClientTime {
        self.now
    }

    fn accept_time(&mut self, now: ClientTime) -> Result<(), ClientTimeError> {
        self.validate_time(now)?;
        self.clock.set_time(now)?;
        self.now = now;
        Ok(())
    }

    /// Close the current transport and discard all buffered data.
    pub fn close(&mut self, now: ClientTime) -> Result<(), ClientTimeError> {
        self.accept_time(now)?;
        self.close_resources();
        Ok(())
    }

    pub(crate) fn close_resources(&mut self) {
        self.cancel_content_jobs();
        self.content = Default::default();
        if let Some(mut transport) = self.transport.take() {
            transport.close();
        }
        self.actor = None;
        self.state = SessionState::Disconnected;
        self.pending_ping_at = None;
        self.pending_ping_frame = None;
        self.world_context = None;
        self.active_world_content = None;
        self.pending_world_content = None;
        self.world_content_job = None;
        self.world_content_result = None;
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
        self.clock.reset(self.now);
    }

    /// Queues an application message. Authentication is reserved for construction;
    /// all other messages are accepted only after Welcome. A full bounded send
    /// buffer is reported without disconnecting so the caller can retry.
    pub fn queue(&mut self, message: &ClientMessage, now: ClientTime) -> Result<(), QueueError> {
        self.accept_time(now).map_err(QueueError::Time)?;
        self.queue_current(message)
    }

    fn queue_current(&mut self, message: &ClientMessage) -> Result<(), QueueError> {
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
        if let ClientMessage::WorldInput { sequence, .. } = message {
            self.metrics.last_queued_world_frame = Some(QueuedWorldFrame {
                sequence: *sequence,
                frame,
            });
        }
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
        now: ClientTime,
    ) -> Result<Option<minimium_protocol::MovementSample>, QueueError> {
        self.accept_time(now).map_err(QueueError::Time)?;
        let context = self.world_context.ok_or(QueueError::AwaitingWelcome)?;
        let mut queued_sample = None;
        let mut motion_counters = None;
        if let minimium_protocol::ClientWorldInput::Jump { input_seq } = &mut input {
            *input_seq = self.next_input_seq;
            motion_counters = Some(
                input_seq
                    .checked_add(1)
                    .ok_or(QueueError::InputSequenceExhausted)?,
            );
        } else if let minimium_protocol::ClientWorldInput::Motion(sample) = &mut input {
            let input_seq = self.next_input_seq;
            let next_input_seq = input_seq
                .checked_add(1)
                .ok_or(QueueError::InputSequenceExhausted)?;
            sample.input_seq = input_seq;
            sample.target_tick = self.server_tick.saturating_add(1);
            motion_counters = Some(next_input_seq);
            queued_sample = Some(*sample);
        }
        let sequence = self.next_world_sequence;
        let next = sequence
            .checked_add(1)
            .ok_or(QueueError::InputSequenceExhausted)?;
        let control_revision = self
            .clock_sample
            .ok_or(QueueError::AwaitingWelcome)?
            .revision()
            .get();
        self.queue_current(&ClientMessage::WorldInput {
            sequence,
            control_revision,
            context,
            input,
        })?;
        self.next_world_sequence = next;
        if let Some(next_input_seq) = motion_counters {
            self.next_input_seq = next_input_seq;
        }
        Ok(queued_sample)
    }
}

impl Drop for ClientSession {
    fn drop(&mut self) {
        self.cancel_content_jobs();
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
