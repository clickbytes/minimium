use crate::{
    admin::{AdminRequestId, AdminResponse, AdminState, AdminTransportEvent},
    game::{self, GameOutput, GameState, GameTransportEvent, TransportSender},
    QueueError, TransportError,
};
use std::{
    io,
    net::{SocketAddr, TcpListener, ToSocketAddrs},
    sync::{Arc, Mutex},
    time::Instant,
};

pub struct TransportConfig {
    pub bind_address: String,
    pub game_port: u16,
    pub admin_token: String,
    pub max_connections: usize,
}

#[derive(Clone, Copy, Default)]
pub struct TransportMetrics {
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub frames_in: u64,
    pub frames_out: u64,
    pub max_client_input_bytes: usize,
    pub max_client_output_bytes: usize,
}

pub enum TransportEvent {
    Game(GameTransportEvent),
    Admin(AdminTransportEvent),
}

#[derive(Clone)]
pub struct TransportEndpoint {
    pub game_address: String,
    pub game_port: u16,
    pub admin_port: u16,
    pub admin_token: String,
}

/// Host transport service consumed by the runtime without socket or codec access.
pub trait TransportPort {
    fn poll(&mut self, now: Instant) -> Result<Vec<TransportEvent>, TransportError>;
    fn flush(&mut self, now: Instant) -> Vec<TransportEvent>;
    fn game_output(&self) -> Arc<dyn crate::GameOutputPort>;
    fn stop_admission(&mut self);
    fn has_pending_admin_output(&self) -> bool;
    fn close_all(&mut self);
    fn respond_admin(
        &mut self,
        id: AdminRequestId,
        response: AdminResponse,
    ) -> Result<(), QueueError>;
    fn reject_admin_pending(&mut self);
    fn is_admin_pending(&self, id: AdminRequestId, now: Instant) -> bool;
    fn metrics(&self) -> TransportMetrics;
    fn endpoint(&self) -> TransportEndpoint;
}

/// Process-host-created owner of all listening and connected transport sockets.
pub struct HostTransport {
    game: Arc<Mutex<GameState>>,
    admin: AdminState,
    endpoint: TransportEndpoint,
    metrics: TransportMetrics,
}

impl HostTransport {
    pub fn bind(config: TransportConfig) -> Result<Self, TransportError> {
        let addresses = (config.bind_address.as_str(), config.game_port)
            .to_socket_addrs()
            .map_err(TransportError::Io)?
            .collect::<Vec<_>>();
        if addresses.is_empty() {
            return Err(TransportError::Io(io::Error::new(
                io::ErrorKind::AddrNotAvailable,
                "bind address resolved to no addresses",
            )));
        }
        let game_listener = TcpListener::bind(addresses.as_slice()).map_err(TransportError::Io)?;
        game_listener
            .set_nonblocking(true)
            .map_err(TransportError::Io)?;
        let game_address = game_listener.local_addr().map_err(TransportError::Io)?;
        let admin_listener = TcpListener::bind(("127.0.0.1", 0)).map_err(TransportError::Io)?;
        admin_listener
            .set_nonblocking(true)
            .map_err(TransportError::Io)?;
        let admin_address = admin_listener.local_addr().map_err(TransportError::Io)?;
        let admin = AdminState::new(admin_listener, config.admin_token.clone())
            .map_err(TransportError::Io)?;
        Ok(Self {
            game: game::new_game_state(game_listener, config.max_connections),
            admin,
            endpoint: TransportEndpoint {
                game_address: game_address.to_string(),
                game_port: game_address.port(),
                admin_port: admin_address.port(),
                admin_token: config.admin_token,
            },
            metrics: TransportMetrics::default(),
        })
    }

    pub fn game_address(&self) -> SocketAddr {
        self.endpoint
            .game_address
            .parse()
            .expect("bound game address is a socket address")
    }
    pub fn admin_address(&self) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], self.endpoint.admin_port))
    }
    pub fn admin_token(&self) -> &str {
        &self.endpoint.admin_token
    }
    pub fn sender(&self) -> TransportSender {
        TransportSender::from_state(self.game.clone())
    }

    pub fn poll(&mut self, now: Instant) -> Result<Vec<TransportEvent>, TransportError> {
        let game_events = game::poll_game(&self.game, now).map_err(|error| match error {
            game::GamePollError::Io(error) => TransportError::Io(error),
            game::GamePollError::IdentifierExhausted => TransportError::IdentifierExhausted,
        })?;
        let admin_events = self.admin.poll(now).map_err(TransportError::Io)?;
        self.refresh_metrics();
        Ok(game_events
            .into_iter()
            .map(TransportEvent::Game)
            .chain(admin_events.into_iter().map(TransportEvent::Admin))
            .collect())
    }

    pub fn flush(&mut self, now: Instant) -> Vec<TransportEvent> {
        let events = game::flush_game(&self.game, now)
            .into_iter()
            .map(TransportEvent::Game)
            .collect();
        self.refresh_metrics();
        events
    }

    pub fn queue_game(&self, output: GameOutput) -> Result<game::QueuedFrame, QueueError> {
        self.sender().queue_game(output)
    }

    pub fn stop_admission(&mut self) {
        self.game
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .listener
            .take();
        self.admin.stop_admission();
    }

    pub fn has_pending_admin_output(&self) -> bool {
        self.admin.has_pending_output()
    }

    pub fn close_all(&mut self) {
        let mut game = self
            .game
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        game.listener.take();
        game.connections.clear();
        game.closed = true;
        self.admin.close_all();
    }

    pub fn respond_admin(
        &mut self,
        id: AdminRequestId,
        response: AdminResponse,
    ) -> Result<(), QueueError> {
        if self.admin.respond(id, response) {
            Ok(())
        } else {
            Err(QueueError::UnknownAdminRequest)
        }
    }

    pub fn reject_admin_pending(&mut self) {
        self.admin.reject_pending();
    }
    pub fn is_admin_pending(&self, id: AdminRequestId, now: Instant) -> bool {
        self.admin.is_pending(id, now)
    }
    pub fn metrics(&self) -> TransportMetrics {
        self.metrics
    }

    fn refresh_metrics(&mut self) {
        let game = self
            .game
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        self.metrics.bytes_in = game.totals.bytes_in;
        self.metrics.bytes_out = game.totals.bytes_out;
        self.metrics.frames_in = game.totals.frames_in;
        self.metrics.frames_out = game.totals.frames_out;
        self.metrics.max_client_input_bytes = game.max_input_bytes;
        self.metrics.max_client_output_bytes = game.max_output_bytes;
    }
}

impl Drop for HostTransport {
    fn drop(&mut self) {
        self.close_all();
    }
}

impl TransportPort for HostTransport {
    fn poll(&mut self, now: Instant) -> Result<Vec<TransportEvent>, TransportError> {
        HostTransport::poll(self, now)
    }
    fn flush(&mut self, now: Instant) -> Vec<TransportEvent> {
        HostTransport::flush(self, now)
    }
    fn game_output(&self) -> Arc<dyn crate::GameOutputPort> {
        Arc::new(self.sender())
    }
    fn stop_admission(&mut self) {
        HostTransport::stop_admission(self);
    }
    fn has_pending_admin_output(&self) -> bool {
        HostTransport::has_pending_admin_output(self)
    }
    fn close_all(&mut self) {
        HostTransport::close_all(self);
    }
    fn respond_admin(
        &mut self,
        id: AdminRequestId,
        response: AdminResponse,
    ) -> Result<(), QueueError> {
        HostTransport::respond_admin(self, id, response)
    }
    fn reject_admin_pending(&mut self) {
        HostTransport::reject_admin_pending(self);
    }
    fn is_admin_pending(&self, id: AdminRequestId, now: Instant) -> bool {
        HostTransport::is_admin_pending(self, id, now)
    }
    fn metrics(&self) -> TransportMetrics {
        HostTransport::metrics(self)
    }
    fn endpoint(&self) -> TransportEndpoint {
        self.endpoint.clone()
    }
}
