use super::*;
use std::net::SocketAddr;

#[derive(Debug, Clone, Copy)]
pub struct SessionTraffic {
    pub session: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub ping_ms: Option<u128>,
    pub ping_age_ms: Option<u128>,
}
#[derive(Debug, Clone, Copy)]
pub struct ConnectionStats {
    pub process_sent: u64,
    pub process_received: u64,
    pub session: Option<SessionTraffic>,
}

/// Accept the socket-address and DNS host:port forms supported by the connector.
pub fn valid_endpoint(endpoint: &str) -> bool {
    if endpoint.parse::<SocketAddr>().is_ok() {
        return true;
    }
    let Some((host, port)) = endpoint.rsplit_once(':') else {
        return false;
    };
    !host.is_empty()
        && host.len() <= 253
        && host.trim_end_matches('.').split('.').all(|l| {
            !l.is_empty()
                && l.len() <= 63
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
        && port.parse::<u16>().is_ok()
}

impl ConnectionController {
    pub fn active_attempt_id(&self) -> Option<u64> {
        self.active.map(AttemptId::get)
    }
    pub fn cleanup_pending(&self) -> bool {
        self.cleanup.is_some()
    }
    pub fn retained_owned_process(&self) -> bool {
        matches!(self.local_server, Some(LocalServer::Owned(_)))
    }

    pub(super) fn retire_session(&mut self) -> Option<ClientSession> {
        let session = self.session.take()?;
        let metrics = session.metrics();
        self.closed_traffic.0 = self.closed_traffic.0.saturating_add(metrics.bytes_sent);
        self.closed_traffic.1 = self.closed_traffic.1.saturating_add(metrics.bytes_received);
        Some(session)
    }

    pub fn traffic(&self, now: ClientTime) -> ConnectionStats {
        let session = self.session.as_ref().map(|s| {
            let m = s.metrics();
            SessionTraffic {
                session: s.content_jobs_session_id(),
                bytes_sent: m.bytes_sent,
                bytes_received: m.bytes_received,
                ping_ms: m.ping_round_trip.map(|d| d.as_millis()),
                ping_age_ms: m.ping_sample_at.map(|at| now.elapsed_since(at).as_millis()),
            }
        });
        ConnectionStats {
            process_sent: self
                .closed_traffic
                .0
                .saturating_add(session.map_or(0, |s| s.bytes_sent)),
            process_received: self
                .closed_traffic
                .1
                .saturating_add(session.map_or(0, |s| s.bytes_received)),
            session,
        }
    }
}
