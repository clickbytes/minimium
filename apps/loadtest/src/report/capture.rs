use minimium_core::{admin_client, endpoint::ServerEndpoint};
use std::{
    net::{Ipv4Addr, SocketAddr},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

pub struct ServerSample {
    pub status: String,
    pub metrics: String,
    identity: EndpointIdentity,
}

#[derive(PartialEq, Eq)]
struct EndpointIdentity {
    game_port: u16,
    admin_port: u16,
    token: String,
}

#[derive(Default)]
pub struct ReportMetadata {
    pub run_id: String,
    pub scenario_id: String,
    pub requested_seconds: u64,
    pub setup_elapsed_seconds: Option<f64>,
    pub address: String,
    pub thread_mode: String,
    pub cpu_label: Option<String>,
    pub cpu_mhz: Option<u32>,
    pub server_before: Option<ServerSample>,
    pub server_measurement_start: Option<ServerSample>,
    pub server_after: Option<ServerSample>,
    server_samples_invalidated: bool,
}

impl ReportMetadata {
    pub fn new(scenario_id: impl Into<String>) -> Self {
        Self {
            run_id: new_run_id(),
            scenario_id: scenario_id.into(),
            ..Self::default()
        }
    }

    pub fn reject_server_samples(&mut self) {
        self.server_samples_invalidated = true;
        self.server_before = None;
        self.server_measurement_start = None;
        self.server_after = None;
    }

    pub fn validate_server_identity(&mut self) -> bool {
        if self.server_samples_invalidated {
            self.reject_server_samples();
            return false;
        }
        let changed = {
            let samples = [
                self.server_before.as_ref(),
                self.server_measurement_start.as_ref(),
                self.server_after.as_ref(),
            ];
            let mut identity: Option<&EndpointIdentity> = None;
            samples.into_iter().flatten().any(|sample| {
                if identity.is_some_and(|expected| expected != &sample.identity) {
                    true
                } else {
                    identity = Some(&sample.identity);
                    false
                }
            })
        };
        if changed {
            self.reject_server_samples();
            return false;
        }
        true
    }
}

/// Capture actual status and metrics, retaining endpoint identity privately
/// in memory for comparison. Call only at run boundaries, never per tick.
pub fn capture_server(data_dir: &Path, configured_address: &str) -> std::io::Result<ServerSample> {
    let configured = configured_address.parse::<SocketAddr>().ok();
    let Some(SocketAddr::V4(configured)) = configured.filter(
        |address| matches!(address, SocketAddr::V4(value) if *value.ip() == Ipv4Addr::LOCALHOST),
    ) else {
        return Err(identity_error(
            "server samples require a literal 127.0.0.1:PORT address",
        ));
    };
    let endpoint = ServerEndpoint::read(data_dir)?;
    let status = admin_client::request(&endpoint, "status")?;
    if status.contains(&endpoint.token) {
        return Err(identity_error("server response contained endpoint token"));
    }
    let actual_address = status
        .split_whitespace()
        .find_map(|field| field.strip_prefix("game_address="))
        .and_then(|value| value.parse::<SocketAddr>().ok());
    if actual_address != Some(SocketAddr::V4(configured)) || endpoint.game_port != configured.port()
    {
        return Err(identity_error("server configured address check failed"));
    }
    let metrics = admin_client::request(&endpoint, "metrics show");
    let current = ServerEndpoint::read(data_dir)?;
    if endpoint.game_port != current.game_port
        || endpoint.admin_port != current.admin_port
        || endpoint.token != current.token
    {
        return Err(identity_error(
            "server endpoint identity consistency could not be verified during capture",
        ));
    }
    let metrics = metrics?;
    if metrics.contains(&endpoint.token) {
        return Err(identity_error("server response contained endpoint token"));
    }
    Ok(ServerSample {
        status,
        metrics,
        identity: EndpointIdentity {
            game_port: endpoint.game_port,
            admin_port: endpoint.admin_port,
            token: endpoint.token,
        },
    })
}

fn identity_error(message: &'static str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Interrupted, message)
}

fn new_run_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{}-{nanos}", std::process::id())
}
