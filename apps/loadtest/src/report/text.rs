use super::{
    data::ReportData,
    encoding::{display_number, json_string},
};

pub(super) fn render(report: &ReportData<'_>) -> String {
    let stats = report.stats;
    let metadata = report.metadata;
    let elapsed = report.elapsed_rate_denominator;
    format!(
        "scenario={} os={} arch={} run_id={} rustc={} build_profile={} scenario_id={} cpu_label={} cpu_mhz={} setup_elapsed_seconds={} generator_address={} requested_seconds={} thread_mode={} workers_actual={} clients_requested={} clients_connected={} pongs={} errors={} rate_limited={} timeouts={} seconds={:.2} pongs_per_second={:.1} moves_sent={} moves_per_second={:.1} shots_sent={} shots_per_second={:.1} shots_hit={} kills={} chat_sent={} chat_received={} chat_sent_per_second={:.1} snapshots_received={} snapshots_per_second={:.1} application_bytes_sent={} application_bytes_received={} setup_application_bytes_sent={} setup_application_bytes_received={} measurement_application_bytes_sent={} measurement_application_bytes_received={} handshake_failures={} disconnects_before_start={} clients_connected_at_start={} disconnects_during_measurement={} clients_connected_at_end={} client_measurements={} rtt_samples={} rtt_min_ms={} rtt_avg_ms={} rtt_p95_ms={} rtt_p99_ms={} rtt_max_ms={} server_before_status={} server_before_metrics={} server_measurement_start_status={} server_measurement_start_metrics={} server_after_status={} server_after_metrics={}",
        report.scenario,
        std::env::consts::OS,
        std::env::consts::ARCH,
        json_string(&metadata.run_id),
        json_string(env!("MINIMIUM_RUSTC_VERSION")),
        json_string(env!("MINIMIUM_BUILD_PROFILE")),
        json_string(&metadata.scenario_id),
        metadata
            .cpu_label
            .as_deref()
            .map_or_else(|| "n/a".to_owned(), json_string),
        metadata
            .cpu_mhz
            .map_or_else(|| "n/a".to_owned(), |value| value.to_string()),
        display_number(metadata.setup_elapsed_seconds),
        json_string(&metadata.address),
        metadata.requested_seconds,
        json_string(&metadata.thread_mode),
        report.workers,
        report.requested_clients,
        stats.connected,
        stats.pongs,
        stats.errors,
        stats.rate_limited,
        stats.timeouts,
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
        client_measurements(report),
        stats.rtt.samples,
        display_number(stats.rtt.minimum_milliseconds()),
        display_number(stats.rtt.average_milliseconds()),
        display_number(stats.rtt.percentile_milliseconds(95)),
        display_number(stats.rtt.percentile_milliseconds(99)),
        display_number(stats.rtt.maximum_milliseconds()),
        server_value(
            metadata
                .server_before
                .as_ref()
                .map(|sample| sample.status.as_str())
        ),
        server_value(
            metadata
                .server_before
                .as_ref()
                .map(|sample| sample.metrics.as_str())
        ),
        server_value(
            metadata
                .server_measurement_start
                .as_ref()
                .map(|sample| sample.status.as_str())
        ),
        server_value(
            metadata
                .server_measurement_start
                .as_ref()
                .map(|sample| sample.metrics.as_str())
        ),
        server_value(
            metadata
                .server_after
                .as_ref()
                .map(|sample| sample.status.as_str())
        ),
        server_value(
            metadata
                .server_after
                .as_ref()
                .map(|sample| sample.metrics.as_str())
        ),
    )
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

fn server_value(value: Option<&str>) -> String {
    value.map_or_else(|| "n/a".to_owned(), json_string)
}
