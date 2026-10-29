use crate::{
    bootstrap::LocalServer,
    connector::{AttemptId, Connector, PollEvent, StartError},
    messages::ClientText as Text,
    options::ThreadLane,
    session::{ClientSession, QueueError, SessionEvent},
};
use minimium_core::identity::PlayerProfileId;
use minimium_core::world::WorldInstanceId;
use minimium_protocol::ClientMessage;
use std::{
    collections::HashMap,
    io,
    path::PathBuf,
    thread::{self, JoinHandle},
    time::Duration,
};

const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) enum ConnectionEvent {
    Idle,
    InProgress { waiting_for_cleanup: bool },
    TimedOut,
    Cancelled,
    SessionStarted,
    SetupFailed(io::Error),
    Failed(io::Error),
}

/// Owns the connector, session and any local process independently of a UI.
pub(crate) struct ConnectionController {
    connector: Connector,
    active: Option<AttemptId>,
    session: Option<ClientSession>,
    local_server: Option<LocalServer>,
    cleanup: Option<JoinHandle<Result<(), String>>>,
    profile: PlayerProfileId,
    fingerprint: u64,
    world_fingerprints: HashMap<WorldInstanceId, u64>,
}

impl ConnectionController {
    pub(crate) fn new(profile: PlayerProfileId, fingerprint: u64) -> Self {
        Self {
            connector: Connector::default(),
            active: None,
            session: None,
            local_server: None,
            cleanup: None,
            profile,
            fingerprint,
            world_fingerprints: HashMap::from([(
                WorldInstanceId::new(1).expect("primary ID is nonzero"),
                fingerprint,
            )]),
        }
    }

    pub(crate) fn set_world_fingerprints(&mut self, fingerprints: HashMap<WorldInstanceId, u64>) {
        self.world_fingerprints = fingerprints;
    }

    pub(crate) fn busy(&self) -> bool {
        self.session.is_some() || self.active.is_some() || self.cleanup.is_some()
    }

    pub(crate) fn session(&self) -> Option<&ClientSession> {
        self.session.as_ref()
    }

    pub(crate) fn session_mut(&mut self) -> Option<&mut ClientSession> {
        self.session.as_mut()
    }

    pub(crate) fn take_session(&mut self) -> Option<ClientSession> {
        self.session.take()
    }

    pub(crate) fn queue(&mut self, message: &ClientMessage) -> Result<(), QueueError> {
        self.session
            .as_mut()
            .ok_or(QueueError::Disconnected)?
            .queue(message)
    }

    pub(crate) fn cancel_transition(&mut self) -> bool {
        self.session
            .as_mut()
            .is_some_and(ClientSession::cancel_pending_transition)
    }

    pub(crate) fn start_remote(&mut self, address: String) -> Result<(), StartError> {
        self.active = Some(self.connector.start(address, ATTEMPT_TIMEOUT)?);
        Ok(())
    }

    pub(crate) fn start_local(
        &mut self,
        data_dir: PathBuf,
        map_path: Option<PathBuf>,
        world_definition_path: Option<PathBuf>,
        lanes: ThreadLane,
    ) -> Result<(), StartError> {
        self.active = Some(self.connector.start_local(
            data_dir,
            map_path,
            world_definition_path,
            lanes,
            ATTEMPT_TIMEOUT,
        )?);
        Ok(())
    }

    pub(crate) fn poll_connector(&mut self) -> ConnectionEvent {
        if self.cleanup.as_ref().is_some_and(JoinHandle::is_finished) {
            if let Some(handle) = self.cleanup.take() {
                match handle.join() {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => eprintln!("{}", Text::LocalServerGuardCleanupFailed(error)),
                    Err(_) => eprintln!("local server cleanup worker panicked"),
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
                if let Some(server) = local_server {
                    self.retain_local_server(server);
                }
                match ClientSession::new(stream, self.profile, self.fingerprint) {
                    Ok(mut session) => {
                        session.set_world_fingerprints(self.world_fingerprints.clone());
                        self.session = Some(session);
                        ConnectionEvent::SessionStarted
                    }
                    Err(error) => ConnectionEvent::SetupFailed(error),
                }
            }
            PollEvent::Failed { attempt_id, error } if self.active == Some(attempt_id) => {
                self.active = None;
                ConnectionEvent::Failed(error)
            }
            _ => ConnectionEvent::Idle,
        }
    }

    pub(crate) fn poll_session(&mut self, events: &mut Vec<SessionEvent>) {
        if let Some(session) = self.session.as_mut() {
            session.poll(events);
        }
    }

    pub(crate) fn cancel(&mut self) {
        if let Some(id) = self.active.take() {
            self.connector.cancel(id);
        }
    }

    pub(crate) fn finish(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.cancel();
        self.connector.finish_on_exit();
        if let Some(mut session) = self.session.take() {
            session.close();
        }
        let cleanup_result = self
            .cleanup
            .take()
            .map_or(Ok(()), |worker| match worker.join() {
                Ok(Ok(())) => Ok(()),
                Ok(Err(error)) => Err(io::Error::other(error)),
                Err(_) => Err(io::Error::other("local server cleanup worker panicked")),
            });
        let server_result = self
            .local_server
            .take()
            .map_or(Ok(()), |mut server| server.shutdown());
        if let Err(error) = &server_result {
            if cleanup_result.is_err() {
                eprintln!("local server shutdown also failed: {error}");
            }
        }
        cleanup_result?;
        server_result
    }

    fn retain_local_server(&mut self, new_server: LocalServer) {
        let replace = match (&mut self.local_server, &new_server) {
            (Some(server @ LocalServer::Owned(_)), LocalServer::Attached) => {
                match server.is_owned_process_running() {
                    Ok(running) => !running,
                    Err(error) => {
                        eprintln!("{}", Text::LocalServerCheckFailed(error.to_string()));
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
                self.cleanup = Some(thread::spawn(move || {
                    old.shutdown().map_err(|error| error.to_string())
                }));
            }
        }
    }
}

impl Drop for ConnectionController {
    fn drop(&mut self) {
        if let Err(error) = self.finish() {
            eprintln!("{}", Text::LocalServerGuardCleanupFailed(error.to_string()));
        }
    }
}
