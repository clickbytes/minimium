use super::DesktopHost;
use crate::environment::SkyDiagnosticsSnapshot;

pub(super) fn report(renderer: &DesktopHost) -> Option<minimium_commands::operation::CommandValue> {
    use super::render_queries::{finite, record};
    use minimium_commands::operation::CommandValue as V;
    let sample = renderer.meteors.snapshot();
    if sample.context.is_none() || sample.context != renderer.render_environment_frame.context {
        return None;
    }
    let gpu = &renderer.renderer.milky_way.meteors;
    let mut fields = vec![
        ("presentation_time_s", finite(sample.presentation_time_s)),
        ("active", V::U64(sample.active as u64)),
        (
            "active_limit",
            V::U64(crate::renderer::meteors::MAX_ACTIVE_METEOR_EVENTS as u64),
        ),
        ("spawned_total", V::U64(sample.spawned_total)),
        ("dismissed_total", V::U64(sample.dismissed_total)),
        ("last_seed", V::U64(sample.last_seed)),
        ("paused", V::Bool(sample.paused)),
        ("automatic_enabled", V::Bool(sample.enabled)),
        ("automatic_rate", finite(sample.rate)),
        ("automatic_spawned", V::U64(sample.automatic_spawned)),
        ("automatic_skipped", V::U64(sample.automatic_skipped)),
        ("automatic_peak", V::U64(sample.automatic_peak as u64)),
        ("automatic_gate", V::Enum(sample.gate.into())),
        ("prepare_cpu_us", V::U64(sample.prepare_us)),
        ("omitted_cores", V::U64(sample.omitted_cores)),
        (
            "gpu_cores",
            V::U64(renderer.renderer.meteors_status.core_count() as u64),
        ),
        ("gpu_segments", V::U64(gpu.count as u64)),
        ("gpu_upload_bytes", V::U64(gpu.upload_bytes as u64)),
        ("gpu_timing", V::Enum("not_instrumented".into())),
        (
            "audio_backend",
            V::Enum(renderer.meteors.audio.backend_status().into()),
        ),
        ("audio_enabled", V::Bool(renderer.meteors.audio.enabled)),
    ];
    if let Some(next) = sample.next_in_presentation_s {
        fields.push(("next_in_presentation_s", finite(next)));
    }
    Some(record(fields))
}

pub(super) fn paths_report(renderer: &DesktopHost) -> minimium_commands::operation::CommandValue {
    use super::render_queries::{finite, record};
    use minimium_commands::operation::CommandValue as V;
    let paths = renderer.meteors.path_snapshots();
    let rows = paths
        .into_iter()
        .take(crate::renderer::meteors::MAX_ACTIVE_METEOR_EVENTS)
        .map(|path| {
            let mut fields = vec![
                ("id", V::U64(path.id)),
                ("preset", V::Enum(path.preset.label().into())),
                ("seed", V::U64(path.seed)),
                ("age_presentation_s", finite(path.age_s)),
                ("phase", V::Enum(path.phase.label().into())),
                ("train_points", V::U64(path.train_points as u64)),
                ("train_overwritten", V::U64(path.train_overwritten)),
            ];
            if let Some(mass) = path.mass_kg {
                fields.push(("mass_kg", finite(mass)));
            }
            if let Some(speed) = path.speed_m_s {
                fields.push(("speed_m_s", finite(speed)));
            }
            if let Some(start) = path.start_phys_m {
                for (name, value) in ["start_x_m", "start_y_m", "start_z_m"]
                    .into_iter()
                    .zip(start)
                {
                    fields.push((name, finite(value)));
                }
            }
            if let Some(velocity) = path.velocity_m_s {
                for (name, value) in ["velocity_x_m_s", "velocity_y_m_s", "velocity_z_m_s"]
                    .into_iter()
                    .zip(velocity)
                {
                    fields.push((name, finite(value)));
                }
            }
            record(fields)
        })
        .collect();
    record([
        (
            "rows",
            V::Page {
                rows,
                next_cursor: None,
            },
        ),
        (
            "limit",
            V::U64(crate::renderer::meteors::MAX_ACTIVE_METEOR_EVENTS as u64),
        ),
        ("truncated", V::Bool(false)),
    ])
}

pub(super) fn push(snapshot: &mut SkyDiagnosticsSnapshot, renderer: &DesktopHost) {
    snapshot.push(
        "transient_light_enabled",
        renderer.renderer.transient_enabled,
    );
    snapshot.push("transient_lights", renderer.renderer.transient_count);
    snapshot.push(
        "transient_observer_irradiance",
        renderer.renderer.transient_incident,
    );
    snapshot.push(
        "transient_buffer_bytes",
        renderer.renderer.transient_buffer.size(),
    );
    snapshot.push(
        "transient_transmission_debug",
        renderer.renderer.transient_transmission,
    );
    snapshot.push(
        "transient_cloud_records",
        renderer.renderer.cloud_gpu.light_records.len(),
    );
    snapshot.push(
        "transient_cloud_proxy_limit",
        if renderer.renderer.cloud_gpu.standard_lighting
            && !renderer.renderer.milky_way.meteors.low_quality
        {
            8
        } else {
            0
        },
    );
    let status = renderer.renderer.meteors_status;
    let gpu = &renderer.renderer.milky_way.meteors;
    snapshot.push(
        "meteors_optical_heads",
        if renderer.renderer.transient_enabled {
            gpu.optical_heads
        } else {
            0
        },
    );
    snapshot.push(
        "meteors_optics",
        if renderer.renderer.atmosphere_gpu.is_active() {
            "bounded Mie halo"
        } else {
            "analytic narrow halo"
        },
    );
    snapshot.push("meteors_segments", gpu.count);
    snapshot.push("meteors_culled_segments", gpu.culled_segments);
    snapshot.push("meteors_quality_dropped_cores", gpu.dropped_cores);
    snapshot.push("meteors_upload_cpu_us", gpu.prepare_us);
    snapshot.push(
        "meteor_cloud_scans",
        renderer.renderer.cloud_gpu.light_cache.scans,
    );
    snapshot.push(
        "meteor_cloud_cache_reuse",
        renderer.renderer.cloud_gpu.light_cache.reused,
    );
    snapshot.push(
        "meteors_gpu_time",
        "n/a: not instrumented; draw count is not timing",
    );
    snapshot.push("meteors_dust_segments", gpu.dust_segments);
    snapshot.push("meteors_emission_segments", gpu.emission_segments);
    snapshot.push("meteors_draw_count", gpu.draw_count.get());
    snapshot.push("meteors_upload_bytes", gpu.upload_bytes);
    snapshot.push("meteors_rejected_below_cloud_gate", gpu.rejected_low);
    snapshot.push(
        "meteors_GPU_buffer_bytes",
        minimium_renderer::meteors_gpu::MeteorGpu::BYTES,
    );
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
