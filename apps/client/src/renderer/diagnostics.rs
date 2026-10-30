use super::Renderer;
use crate::environment::{diagnostics::format_utc_millis, SkyDiagnosticsSnapshot};

impl Renderer {
    pub(super) fn sky_info_lines(&self) -> Vec<String> {
        let frame = &self.render_environment_frame;
        let input = self.environment_frame;
        let mut snapshot = SkyDiagnosticsSnapshot::new();
        snapshot.push(
            "profile",
            frame
                .descriptor
                .map_or("n/a", |value| profile_label(value.profile())),
        );
        snapshot.push("requested_quality", self.sky_quality.requested.label());
        snapshot.push("effective_quality", self.sky_quality.effective.label());
        snapshot.push(
            "fallback_reason",
            self.sky_quality.fallback_reason.unwrap_or("none"),
        );
        snapshot.push_optional(
            "astronomy_model_revision",
            frame
                .descriptor
                .map(|value| value.astronomy_model_revision().get()),
        );
        snapshot.push_optional(
            "catalogue_id",
            frame.descriptor.map(|value| value.catalogue_id().get()),
        );
        snapshot.push(
            "catalogue_sha256",
            self.sky_catalogue
                .as_ref()
                .map(|catalogue| sha256_hex(catalogue.identity().sha256))
                .unwrap_or_else(|| "n/a".to_owned()),
        );
        snapshot.push_optional(
            "catalogue_count",
            self.sky_catalogue
                .as_ref()
                .map(|catalogue| catalogue.records().len()),
        );
        push_clock_fields(&mut snapshot, input);
        push_observer_fields(&mut snapshot, frame);
        push_body_fields(&mut snapshot, frame);
        push_counter_fields(
            &mut snapshot,
            self.metrics.frame_stats.environment_snapshot(),
        );
        push_shadow_fields(&mut snapshot, self);
        push_gpu_fields(&mut snapshot, self);
        snapshot.lines().collect()
    }
}

fn push_clock_fields(
    snapshot: &mut SkyDiagnosticsSnapshot,
    input: crate::environment::EnvironmentFrameInput,
) {
    let clock = input.clock_diagnostics;
    snapshot.push(
        "clock_source",
        clock.map_or("n/a", |value| value.source.label()),
    );
    snapshot.push_optional(
        "clock_mode",
        clock.and_then(|value| value.mode).map(clock_mode_label),
    );
    snapshot.push_optional(
        "clock_id",
        clock
            .and_then(|value| value.clock_id)
            .map(|value| value.get()),
    );
    snapshot.push_optional("clock_revision", input.revision);
    snapshot.push(
        "world_utc",
        input
            .render_world_utc_ms
            .map(format_utc_millis)
            .unwrap_or_else(|| "n/a".to_owned()),
    );
    snapshot.push_optional("rate_ppm", input.rate_ppm);
    snapshot.push(
        "rate",
        input.rate_ppm.map_or_else(
            || "n/a".to_owned(),
            |ppm| format!("{:.6}x", ppm as f64 / 1_000_000.0),
        ),
    );
    snapshot.push_optional(
        "offline_policy",
        clock
            .and_then(|value| value.offline_policy)
            .map(offline_policy_label),
    );
    snapshot.push_optional(
        "sample_age_real_ms",
        clock.and_then(|value| value.sample_age_real_ms),
    );
    snapshot.push(
        "clock_offset_ms",
        clock
            .and_then(|value| value.clock_offset_ms)
            .map_or_else(|| "n/a".to_owned(), |value| format!("{value:.3}")),
    );
    snapshot.push_optional("probe_rtt_ms", clock.and_then(|value| value.probe_rtt_ms));
    snapshot.push("clock_sync_state", input.quality.label());
    snapshot.push_optional(
        "clock_samples_accepted",
        clock.and_then(|value| value.samples_accepted),
    );
    snapshot.push_optional(
        "clock_samples_rejected",
        clock.and_then(|value| value.samples_rejected),
    );
}

fn push_observer_fields(
    snapshot: &mut SkyDiagnosticsSnapshot,
    frame: &super::environment::EnvironmentFrame,
) {
    let observer = frame.observer;
    snapshot.push(
        "observer_lat_deg",
        observer.map_or_else(
            || "n/a".to_owned(),
            |value| format!("{:.6}", value.geographic.latitude_rad().to_degrees()),
        ),
    );
    snapshot.push(
        "observer_lon_deg",
        observer.map_or_else(
            || "n/a".to_owned(),
            |value| format!("{:.6}", value.geographic.longitude_rad().to_degrees()),
        ),
    );
    snapshot.push(
        "observer_height_m",
        observer.map_or_else(
            || "n/a".to_owned(),
            |value| format!("{:.2}", value.geographic.height_m()),
        ),
    );
    snapshot.push(
        "observer_height_policy",
        observer.map_or("n/a", |value| match value.model {
            minimium_core::sky::SkyModel::EarthGeoreferencedV1 => "EarthHeightReferenceV1",
            minimium_core::sky::SkyModel::MappedEarthSkyV1 => "MappedGameHeightV1",
            minimium_core::sky::SkyModel::None => "n/a",
        }),
    );
}

fn push_body_fields(
    snapshot: &mut SkyDiagnosticsSnapshot,
    frame: &super::environment::EnvironmentFrame,
) {
    let sky = frame.sky;
    snapshot.push_optional(
        "sun_alt_deg",
        sky.map(|value| value.sun.geometric_altitude_rad.to_degrees()),
    );
    snapshot.push_optional(
        "sun_apparent_alt_deg",
        sky.map(|value| value.sun.apparent_altitude_rad.to_degrees()),
    );
    snapshot.push_optional(
        "sun_azimuth_deg",
        sky.map(|value| value.sun.azimuth_rad.to_degrees().rem_euclid(360.0)),
    );
    snapshot.push_optional(
        "moon_alt_deg",
        sky.map(|value| value.moon.body.geometric_altitude_rad.to_degrees()),
    );
    snapshot.push_optional(
        "moon_apparent_alt_deg",
        sky.map(|value| value.moon.body.apparent_altitude_rad.to_degrees()),
    );
    snapshot.push_optional(
        "moon_azimuth_deg",
        sky.map(|value| value.moon.body.azimuth_rad.to_degrees().rem_euclid(360.0)),
    );
    snapshot.push_optional(
        "moon_illuminated_fraction",
        sky.map(|value| value.moon.illuminated_fraction),
    );
    snapshot.push_optional(
        "moon_angular_radius_rad",
        sky.map(|value| value.moon.body.angular_radius_rad),
    );
}

fn push_counter_fields(
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
        "local_casters_and_uniform_including_upload_excludes_remote_players",
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

fn push_shadow_fields(snapshot: &mut SkyDiagnosticsSnapshot, renderer: &Renderer) {
    let gpu = &renderer.gpu;
    let enabled = gpu.shadow_gpu.settings.enabled;
    let valid = enabled && gpu.shadow_state.valid;
    snapshot.push(
        "shadow_source",
        if valid {
            gpu.shadow_state
                .source
                .map_or("none", |source| source.label())
        } else {
            "none"
        },
    );
    snapshot.push(
        "shadow_size",
        format!(
            "{}x{}",
            gpu.shadow_gpu.settings.resolution, gpu.shadow_gpu.settings.resolution
        ),
    );
    snapshot.push(
        "shadow_range_m",
        if enabled {
            gpu.shadow_gpu.settings.receiver_radius_m.to_string()
        } else {
            "n/a".to_owned()
        },
    );
    let caster_count = usize::from(gpu.shadow_map_vertex_count > 0)
        + usize::from(gpu.shadow_player_vertex_count > 0)
        + gpu.shadow_state.caster_keys.len();
    snapshot.push_optional("shadow_caster_count", enabled.then_some(caster_count));
    snapshot.push_optional(
        "shadow_coverage_complete",
        enabled.then_some(gpu.shadow_state.valid && gpu.shadow_remote_complete),
    );
    snapshot.push(
        "shadow_incomplete_reason",
        if valid {
            "none"
        } else {
            gpu.shadow_state
                .invalid_reason
                .map_or("not_rendered", |reason| reason.label())
        },
    );
}

fn push_gpu_fields(snapshot: &mut SkyDiagnosticsSnapshot, renderer: &Renderer) {
    let gpu = &renderer.gpu;
    let star_bytes = gpu.stars.buffer.size();
    let uniform_bytes = gpu.environment_buffer.size()
        + gpu.camera_buffer.size()
        + gpu.shadow_gpu.uniform_buffer.size();
    let shadow_texture_bytes = gpu.shadow_gpu.texture_bytes();
    let map_caster_bytes = gpu.shadow_map_buffer.as_ref().map_or(0, wgpu::Buffer::size);
    let player_caster_bytes = gpu
        .shadow_player_buffer
        .as_ref()
        .map_or(0, wgpu::Buffer::size);
    snapshot.push(
        "environment_GPU_estimate_bytes",
        star_bytes + uniform_bytes + shadow_texture_bytes + map_caster_bytes + player_caster_bytes,
    );
    snapshot.push("environment_GPU_star_instance_buffer_bytes", star_bytes);
    snapshot.push("environment_GPU_uniform_bytes", uniform_bytes);
    snapshot.push("environment_GPU_shadow_texture_bytes", shadow_texture_bytes);
    snapshot.push("environment_GPU_map_caster_buffer_bytes", map_caster_bytes);
    snapshot.push(
        "environment_GPU_player_caster_buffer_bytes",
        player_caster_bytes,
    );
    snapshot.push(
        "environment_GPU_shared_sector_buffers",
        "included_in_total_engine_estimate",
    );
    snapshot.push(
        "environment_GPU_measurement",
        "engine_estimate_not_driver_VRAM",
    );
}

fn profile_label(profile: minimium_core::environment::EnvironmentProfile) -> &'static str {
    match profile {
        minimium_core::environment::EnvironmentProfile::Interior => "Interior",
        minimium_core::environment::EnvironmentProfile::Earth => "Earth",
        minimium_core::environment::EnvironmentProfile::EarthLike => "EarthLike",
    }
}

fn clock_mode_label(mode: minimium_core::time::ClockMode) -> &'static str {
    match mode {
        minimium_core::time::ClockMode::RealTime => "real_time",
        minimium_core::time::ClockMode::TimeScale => "time_scale",
    }
}

fn offline_policy_label(policy: minimium_core::time::OfflinePolicy) -> &'static str {
    match policy {
        minimium_core::time::OfflinePolicy::Pause => "pause",
        minimium_core::time::OfflinePolicy::Advance => "advance",
    }
}

fn format_optional<T: ToString>(value: Option<T>) -> String {
    value.map_or_else(|| "n/a".to_owned(), |value| value.to_string())
}

fn format_optional_float(value: Option<f64>) -> String {
    value.map_or_else(|| "n/a".to_owned(), |value| format!("{value:.2}"))
}

fn sha256_hex(bytes: [u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
