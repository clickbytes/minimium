use super::Renderer;
use crate::environment::{SkyDiagnosticsSnapshot, diagnostics::format_utc_millis};

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
        push_milky_way_fields(&mut snapshot, self);
        push_meteor_fields(&mut snapshot, self);
        snapshot.lines().collect()
    }
}

fn push_milky_way_fields(snapshot: &mut SkyDiagnosticsSnapshot, renderer: &Renderer) {
    let frame = &renderer.render_environment_frame;
    let overview = frame.render_view.kind == super::environment::EnvironmentViewKind::Overview;
    let u = &frame.environment_uniform;
    let moon_flux = u.moon_disk_radiance[3].max(0.0) * u.local_moon_rgb_visibility[3];
    let visibility = if overview {
        1.0
    } else {
        u.sky_horizon_rgb_star_visibility[3] / (1.0 + 8.0 * moon_flux)
    };
    let drawn = renderer.gpu.environment_draw_calls.sky > 0;
    snapshot.push(
        "milky_way_state",
        if !drawn {
            "sky_not_drawn"
        } else if visibility <= 0.0 {
            "daylight_suppressed"
        } else if overview {
            "orbital_background"
        } else {
            "night_background"
        },
    );
    snapshot.push(
        "milky_way_visibility_before_extinction_and_clouds",
        visibility,
    );
    snapshot.push(
        "milky_way_texture_bytes",
        minimium_renderer::MilkyWayGpu::BYTES,
    );
    snapshot.push("scene_linear_HDR", renderer.gpu.hdr_enabled);
}

fn push_meteor_fields(snapshot: &mut SkyDiagnosticsSnapshot, renderer: &Renderer) {
    let status = renderer.gpu.meteors_status;
    snapshot.push("meteors_state", status.label());
    snapshot.push("meteors_cores", status.core_count());
    snapshot.push(
        "meteors_contract",
        if status.contract_ok() {
            "ok"
        } else {
            "violation"
        },
    );
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
            minimium_environment::sky::SkyModel::EarthGeoreferencedV1 => "EarthHeightReferenceV1",
            minimium_environment::sky::SkyModel::MappedEarthSkyV1 => "MappedGameHeightV1",
            minimium_environment::sky::SkyModel::None => "n/a",
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

fn push_shadow_fields(snapshot: &mut SkyDiagnosticsSnapshot, renderer: &Renderer) {
    let gpu = &renderer.gpu;
    let enabled = gpu.shadow_gpu.settings.enabled;
    let active_publication = gpu
        .world_geometry
        .publication()
        .map(|publication| publication.stamp);
    let publication = active_publication;
    let valid = enabled && renderer.shadow_state.valid && gpu.shadow_publish_ready;
    snapshot.push(
        "shadow_source",
        renderer
            .shadow_state
            .source
            .map_or("none", |source| source.label()),
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
            renderer.shadow_state.receiver_radius_m.to_string()
        } else {
            "n/a".to_owned()
        },
    );
    snapshot.push_optional(
        "shadow_texel_m",
        (enabled && renderer.shadow_state.texel_m > 0.0).then_some(renderer.shadow_state.texel_m),
    );
    snapshot.push_optional(
        "shadow_scene_revision",
        publication.map(|stamp| stamp.geometry_revision),
    );
    snapshot.push_optional(
        "shadow_origin_generation",
        publication.map(|stamp| stamp.origin_generation),
    );
    snapshot.push_optional(
        "shadow_expected_scene_revision",
        renderer
            .prepared_shadow
            .geometry_stamp
            .map(|stamp| stamp.geometry_revision),
    );
    snapshot.push_optional(
        "shadow_expected_origin_generation",
        renderer
            .prepared_shadow
            .geometry_stamp
            .map(|stamp| stamp.origin_generation),
    );
    let caster_count = usize::from(gpu.shadow_gpu.map_casters().is_some())
        + usize::from(gpu.shadow_gpu.player_casters().is_some())
        + renderer.shadow_state.caster_keys.len();
    snapshot.push_optional("shadow_caster_count", enabled.then_some(caster_count));
    snapshot.push_optional(
        "shadow_coverage_complete",
        enabled.then_some(
            renderer.shadow_state.valid
                && gpu.shadow_publish_ready
                && (gpu.overview_active || gpu.shadow_remote_complete),
        ),
    );
    snapshot.push(
        "shadow_incomplete_reason",
        if valid {
            "none"
        } else if renderer.prepared_shadow.valid
            && renderer.prepared_shadow.geometry_stamp != active_publication
        {
            "stale_context"
        } else if let Some(reason) = renderer.prepared_shadow.invalid_reason {
            reason.label()
        } else {
            match gpu.shadow_upload_status {
                super::shadow_gpu::ShadowUploadStatus::Ready => "not_rendered",
                super::shadow_gpu::ShadowUploadStatus::Pending => "upload_pending",
                super::shadow_gpu::ShadowUploadStatus::StaleContext => "stale_context",
                super::shadow_gpu::ShadowUploadStatus::Failed => "upload_failed",
            }
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
    let map_caster_bytes = gpu.shadow_gpu.map_caster_buffer_bytes();
    let player_caster_bytes = gpu.shadow_gpu.player_caster_buffer_bytes();
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

fn profile_label(profile: minimium_environment::EnvironmentProfile) -> &'static str {
    match profile {
        minimium_environment::EnvironmentProfile::Interior => "Interior",
        minimium_environment::EnvironmentProfile::Earth => "Earth",
        minimium_environment::EnvironmentProfile::EarthLike => "EarthLike",
    }
}

fn clock_mode_label(mode: minimium_environment::time::ClockMode) -> &'static str {
    match mode {
        minimium_environment::time::ClockMode::RealTime => "real_time",
        minimium_environment::time::ClockMode::TimeScale => "time_scale",
    }
}

fn offline_policy_label(policy: minimium_environment::time::OfflinePolicy) -> &'static str {
    match policy {
        minimium_environment::time::OfflinePolicy::Pause => "pause",
        minimium_environment::time::OfflinePolicy::Advance => "advance",
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
