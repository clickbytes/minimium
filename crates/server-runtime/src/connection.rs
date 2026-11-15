use crate::metrics::RuntimeMetrics;
use minimium_core::identity::Principal;
use minimium_core::ids::PlayerId;
use minimium_environment::time::ClockRevision;
use minimium_protocol::WorldContext;
use minimium_protocol::{encode_server_into, RemotePlayers, ServerMessage, MAX_SERVER_FRAME_BYTES};
use minimium_transport as network;
mod output;
use output::account_output;
pub(crate) use output::{
    mark_disconnected, queue_close_response, queue_encoded_frame, queue_encoded_frame_with_reserve,
    queue_message,
};

use std::{
    collections::{BTreeSet, VecDeque},
    net::TcpStream,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

pub(crate) const MAX_QUEUED_OUTPUT_BYTES: usize = 64 * 1024;
pub(crate) const MAX_QUEUED_OUTPUT_FRAMES: usize = 256;
pub(crate) const CLIENT_CLOSE_TIMEOUT: Duration = Duration::from_secs(2);
const TIME_PROBE_WINDOW: Duration = Duration::from_secs(1);
static NEXT_CONNECTION_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug)]
pub(crate) enum CloseReason {
    Quit,
    Rejected,
    PeerEof,
    HandshakeTimeout,
    IdleTimeout,
    InvalidFrame,
    ReadError,
    WriteError,
    Backpressure,
    InvalidSequence,
}

#[derive(Clone, Copy)]
pub(crate) struct Identity {
    pub(crate) player: PlayerId,
    pub(crate) principal: Principal,
}

#[derive(Clone, Copy)]
pub(crate) enum ConnectionPhase {
    AwaitPresentation {
        deadline: Instant,
    },
    AwaitAuthentication {
        deadline: Instant,
    },
    Authenticating {
        attempt: crate::authentication::AuthenticationAttemptId,
        deadline: Instant,
    },
    PreparingContent {
        principal: Principal,
        deadline: Instant,
    },
    Active(Identity),
    Draining {
        identity: Option<Identity>,
        authentication_attempt: Option<crate::authentication::AuthenticationAttemptId>,
        deadline: Instant,
        reason: CloseReason,
    },
}

pub(crate) struct Client {
    connection_id: u64,
    pub(crate) protocol_logger: Option<minimium_diagnostics::DomainLogger>,
    presentation: Option<bool>,
    phase: ConnectionPhase,
    pub(crate) disconnect_reason: Option<CloseReason>,
    pub(crate) stream: TcpStream,
    pub(crate) input: Vec<u8>,
    output: Vec<u8>,
    output_offset: usize,
    output_frames: VecDeque<(usize, Instant)>,
    pub(crate) bytes_in: u64,
    pub(crate) bytes_out: u64,
    pub(crate) frames_in: u64,
    pub(crate) frames_out: u64,
    pub(crate) encoding_us: u64,
    pub(crate) replicated_players: RemotePlayers,
    pub(crate) replication_initialized: bool,
    pub(crate) replicated_components: std::collections::BTreeMap<
        minimium_world::entity::EntityId,
        BTreeSet<minimium_world::entity::ComponentTypeId>,
    >,
    pub(crate) entity_revision: u64,
    pub(crate) entity_sync_initialized: bool,
    pub(crate) entity_sync: Option<PendingEntitySync>,
    pub(crate) next_entity_sync_id: u64,
    pub(crate) connected_at: Instant,
    pub(crate) last_message_at: Instant,
    pub(crate) peer_eof: bool,
    pub(crate) last_fire_at: Option<Instant>,
    pub(crate) last_chat_at: Option<Instant>,
    pub(crate) pending_health_update: Option<u16>,
    time_probe_received_at: [Option<Instant>; 2],
    pub(crate) last_environment_sync_at: Option<Instant>,
    pub(crate) last_environment_context: Option<WorldContext>,
    pub(crate) last_environment_revision: Option<ClockRevision>,
    pub(crate) last_time_probe_rejection_at: Option<Instant>,
}

pub(crate) struct PendingEntitySync {
    pub(crate) context: WorldContext,
    pub(crate) sync_id: u64,
    pub(crate) base_revision: u64,
    pub(crate) target_revision: u64,
    pub(crate) full: bool,
    pub(crate) started: Instant,
    pub(crate) begin_pending: bool,
    pub(crate) operations: VecDeque<minimium_protocol::EntityReplicationOperation>,
    pub(crate) visible_components: std::collections::BTreeMap<
        minimium_world::entity::EntityId,
        BTreeSet<minimium_world::entity::ComponentTypeId>,
    >,
    pub(crate) complete_sent: bool,
}

impl Client {
    pub(crate) fn new(
        stream: TcpStream,
        connected_at: Instant,
        logger: Option<minimium_diagnostics::DomainLogger>,
    ) -> Self {
        Self {
            connection_id: NEXT_CONNECTION_ID.fetch_add(1, Ordering::Relaxed),
            protocol_logger: logger,
            phase: ConnectionPhase::AwaitPresentation {
                deadline: connected_at + crate::CLIENT_HANDSHAKE_TIMEOUT,
            },
            presentation: None,
            disconnect_reason: None,
            stream,
            input: Vec::with_capacity(512),
            output: Vec::new(),
            output_offset: 0,
            output_frames: VecDeque::new(),
            bytes_in: 0,
            bytes_out: 0,
            frames_in: 0,
            frames_out: 0,
            encoding_us: 0,
            replicated_players: RemotePlayers::default(),
            replication_initialized: false,
            replicated_components: std::collections::BTreeMap::new(),
            entity_revision: 0,
            entity_sync_initialized: false,
            entity_sync: None,
            next_entity_sync_id: 0,
            connected_at,
            last_message_at: connected_at,
            peer_eof: false,
            last_fire_at: None,
            last_chat_at: None,
            pending_health_update: None,
            time_probe_received_at: [None; 2],
            last_environment_sync_at: None,
            last_environment_context: None,
            last_environment_revision: None,
            last_time_probe_rejection_at: None,
        }
    }

    pub(crate) const fn connection_id(&self) -> u64 {
        self.connection_id
    }

    pub(crate) fn pending_output_bytes(&self) -> usize {
        self.output.len().saturating_sub(self.output_offset)
    }

    pub(crate) fn oldest_output_age(&self) -> Option<Duration> {
        self.output_frames.front().map(|(_, at)| at.elapsed())
    }
    pub(crate) fn identity(&self) -> Option<Identity> {
        match self.phase {
            ConnectionPhase::AwaitPresentation { .. }
            | ConnectionPhase::AwaitAuthentication { .. }
            | ConnectionPhase::Authenticating { .. }
            | ConnectionPhase::PreparingContent { .. } => None,
            ConnectionPhase::Active(identity) => Some(identity),
            ConnectionPhase::Draining { identity, .. } => identity,
        }
    }

    pub(crate) fn player_id(&self) -> Option<PlayerId> {
        self.identity().map(|identity| identity.player)
    }

    pub(crate) fn set_presentation(&mut self, capability: Option<bool>, deadline: Instant) -> bool {
        if !matches!(self.phase, ConnectionPhase::AwaitPresentation { .. }) {
            return false;
        }
        self.presentation = capability;
        self.phase = ConnectionPhase::AwaitAuthentication { deadline };
        true
    }

    pub(crate) fn awaits_authentication(&self) -> bool {
        matches!(self.phase, ConnectionPhase::AwaitAuthentication { .. })
    }

    pub(crate) fn presentation_capability(&self) -> Option<bool> {
        self.presentation
    }

    pub(crate) fn begin_authentication(
        &mut self,
        attempt: crate::authentication::AuthenticationAttemptId,
        deadline: Instant,
    ) {
        debug_assert!(matches!(
            self.phase,
            ConnectionPhase::AwaitAuthentication { .. }
        ));
        self.phase = ConnectionPhase::Authenticating { attempt, deadline };
    }

    pub(crate) fn authentication_attempt(
        &self,
    ) -> Option<(crate::authentication::AuthenticationAttemptId, Instant)> {
        match self.phase {
            ConnectionPhase::Authenticating { attempt, deadline } => Some((attempt, deadline)),
            _ => None,
        }
    }

    pub(crate) fn authentication_attempt_for_cancel(
        &self,
    ) -> Option<crate::authentication::AuthenticationAttemptId> {
        match self.phase {
            ConnectionPhase::Authenticating { attempt, .. } => Some(attempt),
            ConnectionPhase::Draining {
                authentication_attempt,
                ..
            } => authentication_attempt,
            _ => None,
        }
    }

    pub(crate) fn authentication_complete(
        &mut self,
        principal: Principal,
        deadline: Instant,
    ) -> bool {
        if !matches!(self.phase, ConnectionPhase::Authenticating { .. }) {
            return false;
        }
        self.phase = ConnectionPhase::PreparingContent {
            principal,
            deadline,
        };
        true
    }

    pub(crate) fn pending_principal(&self) -> Option<Principal> {
        match self.phase {
            ConnectionPhase::PreparingContent { principal, .. } => Some(principal),
            _ => None,
        }
    }

    pub(crate) fn activate(&mut self, player: PlayerId, principal: Principal) {
        debug_assert!(matches!(
            self.phase,
            ConnectionPhase::PreparingContent { .. }
        ));
        self.phase = ConnectionPhase::Active(Identity { player, principal });
    }

    pub(crate) fn admission_deadline(&self) -> Option<Instant> {
        match self.phase {
            ConnectionPhase::AwaitPresentation { deadline }
            | ConnectionPhase::AwaitAuthentication { deadline }
            | ConnectionPhase::Authenticating { deadline, .. }
            | ConnectionPhase::PreparingContent { deadline, .. } => Some(deadline),
            ConnectionPhase::Active(_) | ConnectionPhase::Draining { .. } => None,
        }
    }

    pub(crate) fn accept_time_probe(&mut self, now: Instant) -> bool {
        for received_at in &mut self.time_probe_received_at {
            if received_at.is_some_and(|at| now.saturating_duration_since(at) >= TIME_PROBE_WINDOW)
            {
                *received_at = None;
            }
        }
        if let Some(slot) = self
            .time_probe_received_at
            .iter_mut()
            .find(|received_at| received_at.is_none())
        {
            *slot = Some(now);
            true
        } else {
            false
        }
    }

    pub(crate) fn mark_environment_sent(
        &mut self,
        context: WorldContext,
        revision: ClockRevision,
        now: Instant,
    ) {
        self.last_environment_sync_at = Some(now);
        self.last_environment_context = Some(context);
        self.last_environment_revision = Some(revision);
    }

    pub(crate) fn close_deadline(&self) -> Option<Instant> {
        match self.phase {
            ConnectionPhase::Draining { deadline, .. } => Some(deadline),
            _ => None,
        }
    }

    pub(crate) fn begin_close(&mut self, now: Instant, reason: CloseReason) {
        if self.close_deadline().is_none() {
            let authentication_attempt = self.authentication_attempt_for_cancel();
            self.phase = ConnectionPhase::Draining {
                identity: self.identity(),
                authentication_attempt,
                deadline: now + CLIENT_CLOSE_TIMEOUT,
                reason,
            };
        }
    }

    pub(crate) fn reason(&self) -> Option<CloseReason> {
        self.disconnect_reason.or(match self.phase {
            ConnectionPhase::Draining { reason, .. } => Some(reason),
            _ => None,
        })
    }

    pub(crate) fn flush(&mut self, budget: usize) -> network::FlushResult {
        let result = network::flush(
            &mut self.stream,
            &self.output,
            &mut self.output_offset,
            budget,
        );
        account_output(self, result.written);
        result
    }
}

#[cfg(test)]
mod tests;
