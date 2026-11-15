//! Explicit local-only host services: zero remote admission and zero file output.
use minimium_server_runtime::{authentication::*, composition::ContentServicePort, RuntimeLoggers};
use minimium_transport::*;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

pub struct NoContentEndpoint;
impl ContentServicePort for NoContentEndpoint {
    fn material_hash(&self) -> [u8; 32] {
        [0; 32]
    }
    fn http_port(&self) -> u16 {
        0
    }
    fn shutdown(&mut self, _: Duration) -> Vec<String> {
        vec![]
    }
}
pub struct NoRemoteAuthentication;
impl AuthenticationProvider for NoRemoteAuthentication {
    fn begin(&self, _: AuthenticationRequest) -> Result<(), AuthenticationFailure> {
        Err(AuthenticationFailure::new("host.remote_disabled"))
    }
    fn poll(&self, _: AuthenticationAttemptId) -> AuthenticationStatus {
        AuthenticationStatus::Rejected(AuthenticationFailure::new("host.remote_disabled"))
    }
    fn cancel(&self, _: AuthenticationAttemptId) {}
}
pub struct NoNetwork;
impl GameOutputPort for NoNetwork {
    fn queue_game(&self, _: GameOutput) -> Result<QueuedFrame, QueueError> {
        Err(QueueError::UnknownConnection)
    }
    fn pending_output_bytes(&self, _: ConnectionId) -> Option<usize> {
        None
    }
    fn oldest_output_age(&self, _: ConnectionId) -> Option<Duration> {
        None
    }
    fn stats(&self, _: ConnectionId) -> Option<GameConnectionStats> {
        None
    }
    fn close(&self, _: ConnectionId) {}
}
impl TransportPort for NoNetwork {
    fn poll(&mut self, _: Instant) -> Result<Vec<TransportEvent>, TransportError> {
        Ok(vec![])
    }
    fn flush(&mut self, _: Instant) -> Vec<TransportEvent> {
        vec![]
    }
    fn game_output(&self) -> Arc<dyn GameOutputPort> {
        Arc::new(NoNetwork)
    }
    fn stop_admission(&mut self) {}
    fn has_pending_admin_output(&self) -> bool {
        false
    }
    fn close_all(&mut self) {}
    fn respond_admin(&mut self, _: AdminRequestId, _: AdminResponse) -> Result<(), QueueError> {
        Err(QueueError::UnknownAdminRequest)
    }
    fn reject_admin_pending(&mut self) {}
    fn is_admin_pending(&self, _: AdminRequestId, _: Instant) -> bool {
        false
    }
    fn metrics(&self) -> TransportMetrics {
        TransportMetrics::default()
    }
    fn endpoint(&self) -> TransportEndpoint {
        TransportEndpoint {
            game_address: "embedded".into(),
            game_port: 0,
            admin_port: 0,
            admin_token: String::new(),
        }
    }
}
struct DiscardDiagnostics;
impl minimium_diagnostics::DiagnosticSink for DiscardDiagnostics {
    fn emit(&self, _: minimium_diagnostics::DiagnosticEvent) {}
}
pub fn loggers() -> RuntimeLoggers {
    use minimium_diagnostics::{DomainId as D, DomainLogger};
    let sink = Arc::new(DiscardDiagnostics);
    let logger = |domain| DomainLogger::new(domain, sink.clone());
    RuntimeLoggers {
        server: logger(D::ServerRuntime),
        transport: logger(D::Transport),
        environment: logger(D::Environment),
        protocol: logger(D::Protocol),
        world: logger(D::World),
        sectors: logger(D::Sectors),
        simulation: logger(D::Simulation),
    }
}
