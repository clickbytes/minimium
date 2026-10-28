use super::{
    capture::ServerSample,
    data::ReportData,
    encoding::{json_number, json_string},
};

pub(super) fn render(report: &ReportData<'_>) -> String {
    let stats = report.stats;
    let metadata = report.metadata;
    let elapsed = report.elapsed_rate_denominator;
    let mut output = format!(
        "{{\"schema_version\":2,\"scenario\":{},\"os\":{},\"arch\":{},\"clients_requested\":{},\"clients_connected\":{},\"pongs\":{},\"errors\":{},\"rate_limited\":{},\"timeouts\":{},\"workers\":{},\"elapsed_seconds\":{:.3},\"pongs_per_second\":{:.3},\"moves_sent\":{},\"moves_per_second\":{:.3},\"shots_sent\":{},\"shots_per_second\":{:.3},\"shots_hit\":{},\"kills\":{},\"chat_sent\":{},\"chat_received\":{},\"chat_sent_per_second\":{:.3},\"snapshots_received\":{},\"snapshots_per_second\":{:.3},\"application_bytes_sent\":{},\"application_bytes_received\":{},\"setup_application_bytes_sent\":{},\"setup_application_bytes_received\":{},\"measurement_application_bytes_sent\":{},\"measurement_application_bytes_received\":{},\"handshake_failures\":{},\"disconnects_before_start\":{},\"clients_connected_at_start\":{},\"disconnects_during_measurement\":{},\"clients_connected_at_end\":{},\"rtt_samples\":{},\"rtt_min_ms\":{},\"rtt_avg_ms\":{},\"rtt_p95_ms\":{},\"rtt_p99_ms\":{},\"rtt_max_ms\":{}",
        json_string(report.scenario),
        json_string(std::env::consts::OS),
        json_string(std::env::consts::ARCH),
        report.requested_clients,
        stats.connected,
        stats.pongs,
        stats.errors,
        stats.rate_limited,
        stats.timeouts,
        report.workers,
        report.elapsed_seconds,
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
        stats
            .bytes_received
            .saturating_sub(stats.setup_bytes_received),
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
    output.push_str(&format!(
        ",\"client_measurements\":{}",
        client_measurements(report)
    ));
    output.push_str(&format!(
        ",\"run_id\":{},\"rustc\":{},\"build_profile\":{},\"scenario_id\":{},\"cpu_label\":{},\"cpu_mhz\":{},\"setup_elapsed_seconds\":{},\"generator\":{{\"address\":{},\"requested_seconds\":{},\"thread_mode\":{},\"workers_actual\":{},\"clients_requested\":{},\"clients_connected\":{},\"clients_connected_at_start\":{},\"clients_connected_at_end\":{}}},\"measurement_window\":{{\"origin\":\"shared start after Welcome, SetName acknowledgement, and server start sample\",\"setup_elapsed_seconds\":{},\"duration_seconds\":{:.3}}},\"server\":{{\"before\":{},\"measurement_start\":{},\"after\":{}}}}}",
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
        report.workers,
        report.requested_clients,
        stats.connected,
        stats.ready,
        stats.connected_at_end,
        json_number(metadata.setup_elapsed_seconds),
        report.elapsed_seconds,
        server_sample(metadata.server_before.as_ref()),
        server_sample(metadata.server_measurement_start.as_ref()),
        server_sample(metadata.server_after.as_ref()),
    ));
    output
}

fn client_measurements(report: &ReportData<'_>) -> String {
    let mut clients = report
        .client_measurements()
        .iter()
        .take(10_000)
        .collect::<Vec<_>>();
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

fn server_sample(sample: Option<&ServerSample>) -> String {
    match sample {
        Some(sample) => format!(
            "{{\"status\":{},\"metrics\":{}}}",
            json_string(&sample.status),
            json_string(&sample.metrics)
        ),
        None => "null".to_owned(),
    }
}
