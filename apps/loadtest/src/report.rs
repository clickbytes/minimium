//! Schema v2 origins: generator counters and RTT values come from `WorkerStats`;
//! OS/architecture and build metadata come from the Rust process/build script;
//! optional CPU details are user supplied; server samples are authenticated
//! `status` and `metrics show` responses captured before and after each run.

use minimium_core::{admin_client, endpoint::ServerEndpoint};
use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use super::{ClientMeasurement, WorkerStats};

pub struct ServerSample {
    pub status: String,
    pub metrics: String,
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
}

impl ReportMetadata {
    pub fn new(scenario_id: impl Into<String>) -> Self {
        Self {
            run_id: new_run_id(),
            scenario_id: scenario_id.into(),
            ..Self::default()
        }
    }
}

/// Capture actual status and metrics from the running server without retaining
/// or returning its endpoint token. Call only at run boundaries, never per tick.
pub fn capture_server(data_dir: &Path) -> std::io::Result<ServerSample> {
    let endpoint = ServerEndpoint::read(data_dir)?;
    let status = admin_client::request(&endpoint, "status")?;
    let metrics = admin_client::request(&endpoint, "metrics show")?;
    let current = ServerEndpoint::read(data_dir)?;
    if endpoint.game_port != current.game_port
        || endpoint.admin_port != current.admin_port
        || endpoint.token != current.token
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "server endpoint changed while capturing report sample",
        ));
    }
    Ok(ServerSample { status, metrics })
}

pub fn new_run_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{}-{nanos}", std::process::id())
}

pub fn print_report(
    format: &str,
    scenario: &str,
    requested_clients: usize,
    workers: usize,
    elapsed_seconds: f64,
    stats: &WorkerStats,
    metadata: &ReportMetadata,
) {
    if format == "json" {
        print_json(
            scenario,
            requested_clients,
            workers,
            elapsed_seconds,
            stats,
            metadata,
        );
    } else {
        print_text(
            scenario,
            requested_clients,
            workers,
            elapsed_seconds,
            stats,
            metadata,
        );
    }
}

fn print_json(
    scenario: &str,
    requested_clients: usize,
    workers: usize,
    elapsed_seconds: f64,
    stats: &WorkerStats,
    metadata: &ReportMetadata,
) {
    let elapsed = elapsed_seconds.max(0.001);
    let mut report = format!(
        "{{\"schema_version\":2,\"scenario\":{},\"os\":{},\"arch\":{},\"clients_requested\":{requested_clients},\"clients_connected\":{},\"pongs\":{},\"errors\":{},\"rate_limited\":{},\"timeouts\":{},\"workers\":{workers},\"elapsed_seconds\":{elapsed_seconds:.3},\"pongs_per_second\":{:.3},\"moves_sent\":{},\"moves_per_second\":{:.3},\"shots_sent\":{},\"shots_per_second\":{:.3},\"shots_hit\":{},\"kills\":{},\"chat_sent\":{},\"chat_received\":{},\"chat_sent_per_second\":{:.3},\"snapshots_received\":{},\"snapshots_per_second\":{:.3},\"application_bytes_sent\":{},\"application_bytes_received\":{},\"setup_application_bytes_sent\":{},\"setup_application_bytes_received\":{},\"measurement_application_bytes_sent\":{},\"measurement_application_bytes_received\":{},\"handshake_failures\":{},\"disconnects_before_start\":{},\"clients_connected_at_start\":{},\"disconnects_during_measurement\":{},\"clients_connected_at_end\":{},\"rtt_samples\":{},\"rtt_min_ms\":{},\"rtt_avg_ms\":{},\"rtt_p95_ms\":{},\"rtt_p99_ms\":{},\"rtt_max_ms\":{}",
        json_string(scenario),
        json_string(std::env::consts::OS),
        json_string(std::env::consts::ARCH),
        stats.connected,
        stats.pongs,
        stats.errors,
        stats.rate_limited,
        stats.timeouts,
        stats.pongs as f64 / elapsed,
        stats.moves_sent,
        stats.moves_sent as f64 / elapsed,
        stats.shots_sent,
        stats.shots_sent as f64 / elapsed,
        stats.shots_hit,
        stats.kills,
        stats.chat_sent,
        stats.chat_received,
        stats.chat_sent as f64 / elapsed,
        stats.snapshots_received,
        stats.snapshots_received as f64 / elapsed,
        stats.bytes_sent,
        stats.bytes_received,
        stats.setup_bytes_sent,
        stats.setup_bytes_received,
        stats.bytes_sent.saturating_sub(stats.setup_bytes_sent),
        stats.bytes_received.saturating_sub(stats.setup_bytes_received),
        stats.handshake_failures,
        stats.disconnects_before_start,
        stats.ready,
        stats.disconnects_during_measurement,
        stats.connected_at_end,
        stats.rtt.samples,
        json_number(stats.rtt.minimum_milliseconds()),
        json_number(stats.rtt.average_milliseconds()),
        json_number(stats.rtt.percentile_milliseconds(95)),
        json_number(stats.rtt.percentile_milliseconds(99)),
        json_number(stats.rtt.maximum_milliseconds()),
    );
    report.push_str(&format!(
        ",\"client_measurements\":{}",
        json_client_measurements(&stats.per_client)
    ));
    report.push_str(&format!(
        ",\"run_id\":{},\"rustc\":{},\"build_profile\":{},\"scenario_id\":{},\"cpu_label\":{},\"cpu_mhz\":{},\"setup_elapsed_seconds\":{},\"generator\":{{\"address\":{},\"requested_seconds\":{},\"thread_mode\":{},\"workers_actual\":{workers},\"clients_requested\":{requested_clients},\"clients_connected\":{},\"clients_connected_at_start\":{},\"clients_connected_at_end\":{}}},\"measurement_window\":{{\"origin\":\"shared start after Welcome, SetName acknowledgement, and server start sample\",\"setup_elapsed_seconds\":{},\"duration_seconds\":{elapsed_seconds:.3}}},\"server\":{{\"before\":{},\"measurement_start\":{},\"after\":{}}}}}",
        json_string(&metadata.run_id),
        json_string(env!("MINIMIUM_RUSTC_VERSION")),
        json_string(env!("MINIMIUM_BUILD_PROFILE")),
        json_string(&metadata.scenario_id),
        metadata.cpu_label.as_deref().map_or_else(|| "null".to_owned(), json_string),
        metadata.cpu_mhz.map_or_else(|| "null".to_owned(), |value| value.to_string()),
        json_number(metadata.setup_elapsed_seconds),
        json_string(&metadata.address),
        metadata.requested_seconds,
        json_string(&metadata.thread_mode),
        stats.connected,
        stats.ready,
        stats.connected_at_end,
        json_number(metadata.setup_elapsed_seconds),
        json_server_sample(metadata.server_before.as_ref()),
        json_server_sample(metadata.server_measurement_start.as_ref()),
        json_server_sample(metadata.server_after.as_ref()),
    ));
    println!("{report}");
}

fn print_text(
    scenario: &str,
    requested_clients: usize,
    workers: usize,
    elapsed_seconds: f64,
    stats: &WorkerStats,
    metadata: &ReportMetadata,
) {
    let elapsed = elapsed_seconds.max(0.001);
    println!(
        "scenario={scenario} os={} arch={} run_id={} rustc={} build_profile={} scenario_id={} cpu_label={} cpu_mhz={} setup_elapsed_seconds={} generator_address={} requested_seconds={} thread_mode={} workers_actual={workers} clients_requested={requested_clients} clients_connected={} pongs={} errors={} rate_limited={} timeouts={} seconds={elapsed_seconds:.2} pongs_per_second={:.1} moves_sent={} moves_per_second={:.1} shots_sent={} shots_per_second={:.1} shots_hit={} kills={} chat_sent={} chat_received={} chat_sent_per_second={:.1} snapshots_received={} snapshots_per_second={:.1} application_bytes_sent={} application_bytes_received={} setup_application_bytes_sent={} setup_application_bytes_received={} measurement_application_bytes_sent={} measurement_application_bytes_received={} handshake_failures={} disconnects_before_start={} clients_connected_at_start={} disconnects_during_measurement={} clients_connected_at_end={} client_measurements={} rtt_samples={} rtt_min_ms={} rtt_avg_ms={} rtt_p95_ms={} rtt_p99_ms={} rtt_max_ms={} server_before_status={} server_before_metrics={} server_measurement_start_status={} server_measurement_start_metrics={} server_after_status={} server_after_metrics={}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        json_string(&metadata.run_id),
        json_string(env!("MINIMIUM_RUSTC_VERSION")),
        json_string(env!("MINIMIUM_BUILD_PROFILE")),
        json_string(&metadata.scenario_id),
        metadata.cpu_label.as_deref().map_or_else(|| "n/a".to_owned(), json_string),
        metadata.cpu_mhz.map_or_else(|| "n/a".to_owned(), |value| value.to_string()),
        display_number(metadata.setup_elapsed_seconds),
        json_string(&metadata.address),
        metadata.requested_seconds,
        json_string(&metadata.thread_mode),
        stats.connected,
        stats.pongs,
        stats.errors,
        stats.rate_limited,
        stats.timeouts,
        stats.pongs as f64 / elapsed,
        stats.moves_sent,
        stats.moves_sent as f64 / elapsed,
        stats.shots_sent,
        stats.shots_sent as f64 / elapsed,
        stats.shots_hit,
        stats.kills,
        stats.chat_sent,
        stats.chat_received,
        stats.chat_sent as f64 / elapsed,
        stats.snapshots_received,
        stats.snapshots_received as f64 / elapsed,
        stats.bytes_sent,
        stats.bytes_received,
        stats.setup_bytes_sent,
        stats.setup_bytes_received,
        stats.bytes_sent.saturating_sub(stats.setup_bytes_sent),
        stats.bytes_received.saturating_sub(stats.setup_bytes_received),
        stats.handshake_failures,
        stats.disconnects_before_start,
        stats.ready,
        stats.disconnects_during_measurement,
        stats.connected_at_end,
        json_client_measurements(&stats.per_client),
        stats.rtt.samples,
        display_number(stats.rtt.minimum_milliseconds()),
        display_number(stats.rtt.average_milliseconds()),
        display_number(stats.rtt.percentile_milliseconds(95)),
        display_number(stats.rtt.percentile_milliseconds(99)),
        display_number(stats.rtt.maximum_milliseconds()),
        metadata.server_before.as_ref().map_or_else(|| "n/a".to_owned(), |sample| json_string(&sample.status)),
        metadata.server_before.as_ref().map_or_else(|| "n/a".to_owned(), |sample| json_string(&sample.metrics)),
        metadata.server_measurement_start.as_ref().map_or_else(|| "n/a".to_owned(), |sample| json_string(&sample.status)),
        metadata.server_measurement_start.as_ref().map_or_else(|| "n/a".to_owned(), |sample| json_string(&sample.metrics)),
        metadata.server_after.as_ref().map_or_else(|| "n/a".to_owned(), |sample| json_string(&sample.status)),
        metadata.server_after.as_ref().map_or_else(|| "n/a".to_owned(), |sample| json_string(&sample.metrics)),
    );
}

fn json_client_measurements(clients: &[ClientMeasurement]) -> String {
    let mut clients = clients.iter().take(10_000).collect::<Vec<_>>();
    clients.sort_by_key(|client| client.generator_id);
    let entries = clients
        .into_iter()
        .map(|client| {
            format!(
                "{{\"generator_id\":{},\"bytes_sent\":{},\"bytes_received\":{},\"status\":{}}}",
                client.generator_id,
                client.bytes_sent,
                client.bytes_received,
                json_string(if client.disconnected {
                    "disconnected"
                } else {
                    "connected"
                }),
            )
        })
        .collect::<Vec<_>>();
    format!("[{}]", entries.join(","))
}

fn json_server_sample(sample: Option<&ServerSample>) -> String {
    match sample {
        Some(sample) => format!(
            "{{\"status\":{},\"metrics\":{}}}",
            json_string(&sample.status),
            json_string(&sample.metrics)
        ),
        None => "null".to_owned(),
    }
}

fn json_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);
    escaped.push('"');
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character <= '\u{1f}' => {
                escaped.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => escaped.push(character),
        }
    }
    escaped.push('"');
    escaped
}

fn json_number(value: Option<f64>) -> String {
    value.map_or_else(|| "null".to_owned(), |number| format!("{number:.3}"))
}

fn display_number(value: Option<f64>) -> String {
    value.map_or_else(|| "n/a".to_owned(), |number| format!("{number:.3}"))
}
