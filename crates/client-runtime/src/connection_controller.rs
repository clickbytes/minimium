mod shutdown;
mod time;

use crate::session::{ClientSession, QueueError, SessionEvent, SessionState};
use crate::ControllerError;
use crate::{
    bootstrap::{LocalServer, LocalServerLaunchConfig},
    connector::{AttemptId, Connector, PollEvent, StartError},
};
use crate::{
    ClientTime, ClientTimeError, ContentJobsPort, ContentJobsSession, ContentShutdownStatus,
};
use minimium_platform::admin_client;
use minimium_protocol::ClientMessage;
use std::{
    io,
    num::NonZeroU64,
    sync::Arc,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(10);
const LOCAL_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(35);

pub enum ConnectionEvent {
    Idle,
    InProgress { waiting_for_cleanup: bool },
    TimedOut,
    Cancelled,
    SessionStarted,
    SetupFailed(io::Error),
    Failed(io::Error),
}

/// Owns the connector, session and any local process independently of a UI.
pub struct ConnectionController {
    now: ClientTime,
    presentation: Option<bool>,
    pub movement: minimium_simulation::MovementPolicy,
    pub flight_enabled: bool,
    connector: Connector,
    active: Option<AttemptId>,
    session: Option<ClientSession>,
    local_server: Option<LocalServer>,
    session_owns_server: bool,
    admin_request: Option<JoinHandle<Result<String, ControllerError>>>,
    cleanup: Option<JoinHandle<io::Result<()>>>,
    authentication: Option<crate::authentication::ClientAuthentication>,
    content_jobs: Option<Arc<dyn ContentJobsPort>>,
    content_jobs_shutdown: bool,
    diagnostic_logger: Option<minimium_diagnostics::DomainLogger>,
    replica_schemas: Vec<(
        minimium_world::entity::ComponentTypeId,
        minimium_world::entity::SchemaVersion,
    )>,
}

impl ConnectionController {
    pub fn new(now: ClientTime) -> Result<Self, ClientTimeError> {
        now.validate_after(now)?;
        Ok(Self {
            now,
            presentation: None,
            movement: Default::default(),
            flight_enabled: false,
            connector: Connector::new(now)?,
            active: None,
            session: None,
            local_server: None,
            session_owns_server: false,
            admin_request: None,
            cleanup: None,
            authentication: None,
            content_jobs: None,
            content_jobs_shutdown: false,
            diagnostic_logger: None,
            replica_schemas: Vec::new(),
        })
    }

    pub fn current_time(&self) -> ClientTime {
        self.now
    }

    pub fn validate_time(&self, now: ClientTime) -> Result<(), ClientTimeError> {
        now.validate_after(self.now)?;
        self.connector.validate_time(now)?;
        if let Some(session) = self.session.as_ref() {
            session.validate_time(now)?;
        }
        Ok(())
    }

    fn accept_time(&mut self, now: ClientTime) -> Result<(), ClientTimeError> {
        self.validate_time(now)?;
        self.now = now;
        Ok(())
    }

    pub fn set_diagnostic_logger(&mut self, logger: minimium_diagnostics::DomainLogger) {
        self.diagnostic_logger = Some(logger);
    }

    pub fn register_replica_schema(
        &mut self,
        kind: minimium_world::entity::ComponentTypeId,
        schema: minimium_world::entity::SchemaVersion,
    ) -> Result<(), crate::ReplicaError> {
        if self
            .replica_schemas
            .iter()
            .any(|(registered, value)| *registered == kind && *value != schema)
        {
            return Err(crate::ReplicaError::SchemaConflict);
        }
        if !self
            .replica_schemas
            .iter()
            .any(|(registered, _)| *registered == kind)
        {
            self.replica_schemas.push((kind, schema));
        }
        Ok(())
    }

    pub fn record_diagnostic_failure(&self, code: &'static str, reason: impl std::fmt::Display) {
        crate::diagnostics::emit(
            self.diagnostic_logger.as_ref(),
            minimium_diagnostics::Level::Error,
            code,
            [crate::diagnostics::field("reason", reason.to_string())],
        );
    }

    /// Installs one service before the first connection attempt. Keeping it for
    /// the controller lifetime preserves IDs and resources across reconnects.
    pub fn set_content_jobs(
        &mut self,
        content_jobs: Arc<dyn ContentJobsPort>,
    ) -> Result<(), StartError> {
        if self.content_jobs.is_some() || self.busy() {
            return Err(StartError::Busy {
                active_attempt: self.active,
            });
        }
        self.content_jobs = Some(content_jobs);
        self.content_jobs_shutdown = false;
        Ok(())
    }

    pub fn set_presentation(&mut self, compressed: bool) {
        self.presentation = Some(compressed);
    }

    pub fn set_authentication(
        &mut self,
        authentication: crate::authentication::ClientAuthentication,
    ) {
        self.authentication = Some(authentication);
    }

    pub fn busy(&self) -> bool {
        self.session.is_some() || self.active.is_some() || self.cleanup.is_some()
    }

    pub fn session(&self) -> Option<&ClientSession> {
        self.session.as_ref()
    }

    pub fn awaiting_owned_welcome(&self) -> bool {
        self.session_owns_server
            && self
                .session
                .as_ref()
                .is_some_and(|session| session.state() == SessionState::AwaitingWelcome)
    }

    pub fn session_mut(&mut self) -> Option<&mut ClientSession> {
        self.session.as_mut()
    }

    pub fn take_session(
        &mut self,
        now: ClientTime,
    ) -> Result<Option<ClientSession>, ClientTimeError> {
        self.accept_time(now)?;
        self.session_owns_server = false;
        Ok(self.session.take())
    }

    /// Sends a server-authoritative time change without blocking the UI loop.
    pub fn queue(&mut self, message: &ClientMessage, now: ClientTime) -> Result<(), QueueError> {
        self.accept_time(now).map_err(QueueError::Time)?;
        self.session
            .as_mut()
            .ok_or(QueueError::Disconnected)?
            .queue(message, now)
    }

    pub fn queue_world_input(
        &mut self,
        input: minimium_protocol::ClientWorldInput,
        now: ClientTime,
    ) -> Result<(), QueueError> {
        self.accept_time(now).map_err(QueueError::Time)?;
        self.session
            .as_mut()
            .ok_or(QueueError::Disconnected)?
            .queue_world_input(input, now)
            .map(|_| ())
    }

    pub fn cancel_transition(&mut self, now: ClientTime) -> Result<bool, ClientTimeError> {
        self.accept_time(now)?;
        self.session
            .as_mut()
            .map_or(Ok(false), |session| session.cancel_pending_transition(now))
    }

    /// Reports that the host has prepared presentation for this transfer.
    pub fn report_transfer_host_ready(
        &mut self,
        ticket: crate::session::TransferTicket,
        now: ClientTime,
    ) -> Result<(), QueueError> {
        self.accept_time(now).map_err(QueueError::Time)?;
        self.session
            .as_mut()
            .ok_or(QueueError::Disconnected)?
            .report_transfer_host_ready(ticket, now)
    }

    pub fn host_readiness_requested(&self, ticket: crate::session::TransferTicket) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| session.host_readiness_requested(ticket))
    }

    pub fn start_remote(&mut self, address: String, now: ClientTime) -> Result<(), StartError> {
        self.accept_time(now).map_err(StartError::Time)?;
        if self.busy() {
            return Err(StartError::Busy {
                active_attempt: self.active,
            });
        }
        self.active = Some(self.connector.start(address, ATTEMPT_TIMEOUT, now)?);
        Ok(())
    }

    pub fn start_local(
        &mut self,
        mut config: LocalServerLaunchConfig,
        now: ClientTime,
    ) -> Result<(), StartError> {
        self.accept_time(now).map_err(StartError::Time)?;
        if self.busy() {
            return Err(StartError::Busy {
                active_attempt: self.active,
            });
        }
        config.diagnostic_logger = self.diagnostic_logger.clone();
        self.active = Some(
            self.connector
                .start_local(config, LOCAL_ATTEMPT_TIMEOUT, now)?,
        );
        Ok(())
    }

    pub fn poll_connector(&mut self, now: ClientTime) -> Result<ConnectionEvent, ClientTimeError> {
        self.accept_time(now)?;
        Ok(self.poll_connector_current(now))
    }

    fn poll_connector_current(&mut self, now: ClientTime) -> ConnectionEvent {
        if self.cleanup.as_ref().is_some_and(JoinHandle::is_finished) {
            if let Some(handle) = self.cleanup.take() {
                match handle.join() {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => crate::diagnostics::emit(
                        self.diagnostic_logger.as_ref(),
                        minimium_diagnostics::Level::Error,
                        "client_runtime.local_server_cleanup_failed",
                        [crate::diagnostics::field("reason", error)],
                    ),
                    Err(_) => crate::diagnostics::emit(
                        self.diagnostic_logger.as_ref(),
                        minimium_diagnostics::Level::Error,
                        "client_runtime.local_server_cleanup_panicked",
                        [],
                    ),
                }
            }
        }
        match self
            .connector
            .poll(now)
            .expect("controller prevalidated connector time")
        {
            PollEvent::Idle => {
                self.active = None;
                ConnectionEvent::Idle
            }
            PollEvent::InProgress {
                attempt_id,
                discarding_result,
            } => {
                let waiting_for_cleanup = discarding_result && self.active != Some(attempt_id);
                self.active = Some(attempt_id);
                ConnectionEvent::InProgress {
                    waiting_for_cleanup,
                }
            }
            PollEvent::TimedOut { attempt_id } if self.active == Some(attempt_id) => {
                ConnectionEvent::TimedOut
            }
            PollEvent::Cancelled { attempt_id } if self.active == Some(attempt_id) => {
                self.active = None;
                ConnectionEvent::Cancelled
            }
            PollEvent::Connected {
                attempt_id,
                stream,
                local_server,
            } if self.active == Some(attempt_id) => {
                self.active = None;
                self.session_owns_server = local_server
                    .as_ref()
                    .is_some_and(|server| matches!(server, LocalServer::Owned(_)));
                if let Some(server) = local_server {
                    self.retain_local_server(server);
                }
                let Some(authentication) = self.authentication.clone() else {
                    self.session_owns_server = false;
                    return ConnectionEvent::SetupFailed(io::Error::other(
                        "client authentication was not configured",
                    ));
                };
                let Some(content_jobs) = self.content_jobs.as_ref() else {
                    self.session_owns_server = false;
                    return ConnectionEvent::SetupFailed(io::Error::other(
                        "client content service was not configured",
                    ));
                };
                let content_jobs = ContentJobsSession::new(
                    NonZeroU64::new(attempt_id.get()).expect("attempt IDs are checked nonzero"),
                    content_jobs.clone(),
                );
                let session = minimium_transport::client::TcpClientTransport::new(stream).and_then(
                    |transport| {
                        ClientSession::with_presentation(
                            Box::new(transport),
                            authentication,
                            self.presentation,
                            content_jobs,
                            now,
                        )
                        .map_err(io::Error::other)
                    },
                );
                match session {
                    Ok(mut session) => {
                        session.set_diagnostic_logger(self.diagnostic_logger.clone());
                        for (kind, schema) in &self.replica_schemas {
                            if let Err(error) = session.register_replica_schema(*kind, *schema) {
                                self.session_owns_server = false;
                                return ConnectionEvent::SetupFailed(io::Error::other(format!(
                                    "replica schema configuration failed: {error:?}"
                                )));
                            }
                        }
                        self.session = Some(session);
                        ConnectionEvent::SessionStarted
                    }
                    Err(error) => {
                        self.session_owns_server = false;
                        ConnectionEvent::SetupFailed(error)
                    }
                }
            }
            PollEvent::Failed { attempt_id, error } if self.active == Some(attempt_id) => {
                self.active = None;
                ConnectionEvent::Failed(error)
            }
            _ => ConnectionEvent::Idle,
        }
    }

    pub fn poll_session(
        &mut self,
        events: &mut Vec<SessionEvent>,
        now: ClientTime,
    ) -> Result<(), ClientTimeError> {
        self.accept_time(now)?;
        let first_new_event = events.len();
        if let Some(session) = self.session.as_mut() {
            session.poll(events, now)?;
        }
        for event in &events[first_new_event..] {
            if let SessionEvent::Message(minimium_protocol::ServerMessage::MovementPolicy(policy)) =
                event
            {
                self.movement = *policy;
                self.flight_enabled = policy.can_fly;
            }
        }
        Ok(())
    }

    fn retain_local_server(&mut self, new_server: LocalServer) {
        let replace = match (&mut self.local_server, &new_server) {
            (Some(server @ LocalServer::Owned(_)), LocalServer::Attached) => {
                match server.is_owned_process_running() {
                    Ok(running) => !running,
                    Err(error) => {
                        crate::diagnostics::emit(
                            self.diagnostic_logger.as_ref(),
                            minimium_diagnostics::Level::Error,
                            "client_runtime.local_server_process_check_failed",
                            [crate::diagnostics::field("reason", error)],
                        );
                        false
                    }
                }
            }
            _ => true,
        };
        if replace {
            if let Some(mut old) = self.local_server.replace(new_server) {
                if matches!(old, LocalServer::Attached) {
                    return;
                }
                // At most one cleanup can be outstanding: busy() prevents a rehost.
                debug_assert!(self.cleanup.is_none());
                self.cleanup = Some(thread::spawn(move || old.shutdown()));
            }
        }
    }
}
