mod time;

use crate::session::{ClientSession, QueueError, SessionEvent, SessionState};
use crate::world_content::WorldContentResolver;
use crate::ControllerError;
use crate::{
    bootstrap::{LocalServer, LocalServerLaunchConfig},
    connector::{AttemptId, Connector, PollEvent, StartError},
};
use minimium_platform::admin_client;
use minimium_protocol::ClientMessage;
use std::{
    io,
    path::PathBuf,
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
    content_cache_dir: Option<PathBuf>,
    content_cache_limit_bytes: u64,
    world_content_resolver: Option<Arc<dyn WorldContentResolver>>,
    diagnostic_logger: Option<minimium_diagnostics::DomainLogger>,
    replica_schemas: Vec<(
        minimium_world::entity::ComponentTypeId,
        minimium_world::entity::SchemaVersion,
    )>,
}

impl ConnectionController {
    pub fn new() -> Self {
        Self {
            presentation: None,
            movement: Default::default(),
            flight_enabled: false,
            connector: Connector::default(),
            active: None,
            session: None,
            local_server: None,
            session_owns_server: false,
            admin_request: None,
            cleanup: None,
            authentication: None,
            content_cache_dir: None,
            content_cache_limit_bytes: minimium_asset_io::DEFAULT_CACHE_LIMIT_BYTES,
            world_content_resolver: None,
            diagnostic_logger: None,
            replica_schemas: Vec::new(),
        }
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

    pub fn set_world_content_resolver(&mut self, resolver: Arc<dyn WorldContentResolver>) {
        self.world_content_resolver = Some(resolver);
    }

    pub fn set_presentation(&mut self, compressed: bool) {
        self.presentation = Some(compressed);
    }

    pub fn set_content_cache_dir(&mut self, cache_dir: PathBuf) {
        self.content_cache_dir = Some(cache_dir);
    }

    pub fn set_content_cache_limit(&mut self, maximum_bytes: u64) {
        self.content_cache_limit_bytes = maximum_bytes.max(1);
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

    pub fn take_session(&mut self) -> Option<ClientSession> {
        self.session_owns_server = false;
        self.session.take()
    }

    /// Sends a server-authoritative time change without blocking the UI loop.
    pub fn queue(&mut self, message: &ClientMessage) -> Result<(), QueueError> {
        self.session
            .as_mut()
            .ok_or(QueueError::Disconnected)?
            .queue(message)
    }

    pub fn queue_world_input(
        &mut self,
        input: minimium_protocol::ClientWorldInput,
    ) -> Result<(), QueueError> {
        self.session
            .as_mut()
            .ok_or(QueueError::Disconnected)?
            .queue_world_input(input)
            .map(|_| ())
    }

    pub fn cancel_transition(&mut self) -> bool {
        self.session
            .as_mut()
            .is_some_and(ClientSession::cancel_pending_transition)
    }

    pub fn start_remote(&mut self, address: String) -> Result<(), StartError> {
        self.active = Some(self.connector.start(address, ATTEMPT_TIMEOUT)?);
        Ok(())
    }

    pub fn start_local(&mut self, mut config: LocalServerLaunchConfig) -> Result<(), StartError> {
        config.diagnostic_logger = self.diagnostic_logger.clone();
        self.active = Some(self.connector.start_local(config, LOCAL_ATTEMPT_TIMEOUT)?);
        Ok(())
    }

    pub fn poll_connector(&mut self) -> ConnectionEvent {
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
        match self.connector.poll() {
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
                match ClientSession::with_presentation(stream, authentication, self.presentation) {
                    Ok(mut session) => {
                        session.set_content_cache_dir(self.content_cache_dir.clone());
                        session.set_content_cache_limit(self.content_cache_limit_bytes);
                        session.set_diagnostic_logger(self.diagnostic_logger.clone());
                        for (kind, schema) in &self.replica_schemas {
                            if let Err(error) = session.register_replica_schema(*kind, *schema) {
                                self.session_owns_server = false;
                                return ConnectionEvent::SetupFailed(io::Error::other(format!(
                                    "replica schema configuration failed: {error:?}"
                                )));
                            }
                        }
                        session.set_world_content_resolver(
                            attempt_id.get(),
                            self.world_content_resolver.clone(),
                        );
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

    pub fn poll_session(&mut self, events: &mut Vec<SessionEvent>) {
        let first_new_event = events.len();
        if let Some(session) = self.session.as_mut() {
            session.poll(events);
        }
        for event in &events[first_new_event..] {
            if let SessionEvent::Message(minimium_protocol::ServerMessage::MovementPolicy(policy)) =
                event
            {
                self.movement = *policy;
                self.flight_enabled = policy.can_fly;
            }
        }
    }

    pub fn cancel(&mut self) {
        if let Some(id) = self.active.take() {
            self.connector.cancel(id);
        }
    }

    pub fn finish(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.finish_until(Instant::now() + Duration::from_secs(28))
    }

    pub fn finish_until(&mut self, deadline: Instant) -> Result<(), Box<dyn std::error::Error>> {
        self.cancel();
        let mut unfinished = self.connector.finish_on_exit_until(deadline);
        if let Some(mut session) = self.session.take() {
            session.close();
        }
        let cleanup_result = self.cleanup.take().map_or(Ok(()), |worker| {
            wait_for_cleanup(worker, deadline, &mut unfinished)
        });
        let server_result = self.local_server.take().map_or(Ok(()), |mut server| {
            let worker = thread::spawn(move || server.shutdown());
            wait_for_cleanup(worker, deadline, &mut unfinished)
        });
        if let Err(error) = &server_result {
            if cleanup_result.is_err() {
                crate::diagnostics::emit(
                    self.diagnostic_logger.as_ref(),
                    minimium_diagnostics::Level::Error,
                    "client_runtime.local_server_shutdown_failed",
                    [crate::diagnostics::field("reason", error)],
                );
            }
        }
        cleanup_result?;
        if !unfinished.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "client shutdown exceeded deadline: {}",
                    unfinished.join(",")
                ),
            )
            .into());
        }
        server_result.map_err(Into::into)
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

fn wait_for_cleanup<T: Send + 'static>(
    worker: JoinHandle<io::Result<T>>,
    deadline: Instant,
    unfinished: &mut Vec<String>,
) -> io::Result<T> {
    let name = worker
        .thread()
        .name()
        .unwrap_or("client-cleanup")
        .to_owned();
    let worker = worker;
    while !worker.is_finished() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    if worker.is_finished() {
        return match worker.join() {
            Ok(result) => result,
            Err(_) => Err(io::Error::other("client cleanup worker panicked")),
        };
    }
    unfinished.push(name);
    drop(worker);
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        "client cleanup worker exceeded shutdown deadline",
    ))
}

impl Drop for ConnectionController {
    fn drop(&mut self) {
        if let Err(error) = self.finish() {
            crate::diagnostics::emit(
                self.diagnostic_logger.as_ref(),
                minimium_diagnostics::Level::Error,
                "client_runtime.local_server_guard_cleanup_failed",
                [crate::diagnostics::field("reason", error)],
            );
        }
    }
}
