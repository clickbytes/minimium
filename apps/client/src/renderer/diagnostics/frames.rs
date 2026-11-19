use crate::environment::SkyDiagnosticsSnapshot;
pub(super) fn push_counter_fields(
    snapshot: &mut SkyDiagnosticsSnapshot,
    stats: crate::stats::environment::EnvironmentStatsSnapshot,
) {
    snapshot.push("ephemeris_evaluations", stats.ephemeris_evaluations);
    snapshot.push("ephemeris_cache_hits", stats.ephemeris_cache_hits);
    snapshot.push("star_epoch_updates", stats.star_epoch_updates);
    snapshot.push("star_epoch_cache_hits", stats.star_epoch_cache_hits);
    snapshot.push(
        "environment_cpu_us_avg",
        format_optional_float(stats.cpu.average_us),
    );
    snapshot.push("environment_cpu_us_p95", format_optional(stats.cpu.p95_us));
    snapshot.push("environment_cpu_us_p50", format_optional(stats.cpu.p50_us));
    snapshot.push("environment_cpu_us_p99", format_optional(stats.cpu.p99_us));
    snapshot.push(
        "environment_cpu_us_max",
        format_optional_float(stats.cpu.max_us),
    );
    snapshot.push("environment_cpu_samples", stats.cpu.samples);
    snapshot.push(
        "environment_cpu_scope",
        "observer_astronomy_frame_star_preparation",
    );
    snapshot.push(
        "environment_upload_cpu_us_avg",
        format_optional_float(stats.upload_cpu.average_us),
    );
    snapshot.push(
        "environment_upload_cpu_us_p50",
        format_optional(stats.upload_cpu.p50_us),
    );
    snapshot.push(
        "environment_upload_cpu_us_p95",
        format_optional(stats.upload_cpu.p95_us),
    );
    snapshot.push(
        "environment_upload_cpu_us_p99",
        format_optional(stats.upload_cpu.p99_us),
    );
    snapshot.push(
        "environment_upload_cpu_us_max",
        format_optional_float(stats.upload_cpu.max_us),
    );
    snapshot.push("environment_upload_cpu_samples", stats.upload_cpu.samples);
    snapshot.push(
        "environment_upload_cpu_scope",
        "queue_writes_stars_camera_environment",
    );
    snapshot.push(
        "shadow_prepare_cpu_us_avg",
        format_optional_float(stats.shadow_cpu.average_us),
    );
    snapshot.push(
        "shadow_prepare_cpu_us_p50",
        format_optional(stats.shadow_cpu.p50_us),
    );
    snapshot.push(
        "shadow_prepare_cpu_us_p95",
        format_optional(stats.shadow_cpu.p95_us),
    );
    snapshot.push(
        "shadow_prepare_cpu_us_p99",
        format_optional(stats.shadow_cpu.p99_us),
    );
    snapshot.push(
        "shadow_prepare_cpu_us_max",
        format_optional_float(stats.shadow_cpu.max_us),
    );
    snapshot.push("shadow_prepare_cpu_samples", stats.shadow_cpu.samples);
    snapshot.push(
        "shadow_prepare_cpu_scope",
        "sector_snapshot_caster_selection_map_geometry_and_uniform_cpu_excludes_gpu_upload_and_remote_players",
    );
    snapshot.push("environment_gpu_elapsed_us", "n/a");
    snapshot.push("draw_calls_sky", stats.sky_draw_calls);
    snapshot.push("draw_calls_stars", stats.star_draw_calls);
    snapshot.push("draw_calls_sun", stats.sun_draw_calls);
    snapshot.push("draw_calls_moon", stats.moon_draw_calls);
    snapshot.push("draw_calls_halos", stats.halo_draw_calls);
    snapshot.push("draw_calls_shadows", stats.shadow_draw_calls);
    snapshot.push("render_frames_skipped", stats.frames_skipped);
}

fn format_optional<T: ToString>(value: Option<T>) -> String {
    value.map_or_else(|| "n/a".to_owned(), |value| value.to_string())
}

fn format_optional_float(value: Option<f64>) -> String {
    value.map_or_else(|| "n/a".to_owned(), |value| format!("{value:.2}"))
}
