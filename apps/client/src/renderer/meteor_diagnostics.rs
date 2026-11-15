use super::Renderer;
use crate::environment::SkyDiagnosticsSnapshot;

pub(super) fn status(renderer: &mut Renderer) {
    use crate::client_commands::{ClientPropertyCommand as Command, ClientPropertyKey as Key};
    for key in [
        Key::MeteorEnabled,
        Key::MeteorRate,
        Key::MeteorQuality,
        Key::MeteorSeed,
        Key::MeteorAudio,
    ] {
        let line = renderer.apply_property(Command::Get(key));
        renderer.application.ui.console.push_line(line);
    }
    let gpu = &renderer.gpu.milky_way.meteors;
    renderer.application.ui.console.push_line(format!("meteor upload CPU {} us; culled segments {}; quality-dropped cores {}; cloud scans {} / reused {}; proxy cache {} bytes", gpu.prepare_us, gpu.culled_segments, gpu.dropped_cores, renderer.gpu.cloud_gpu.light_cache.scans, renderer.gpu.cloud_gpu.light_cache.reused, std::mem::size_of::<minimium_renderer::transient_cloud_cache::CloudLightCache>()));
    renderer.application.ui.console.push_line(format!(
        "meteor GPU: cores {}, segments {}, dust {}, culled low {}, upload {} bytes, persistent {} bytes, lights {}",
        renderer.gpu.meteors_status.core_count(), gpu.count, gpu.dust_segments,
        gpu.rejected_low, gpu.upload_bytes,
        minimium_renderer::meteors_gpu::MeteorGpu::BYTES + renderer.gpu.transient_buffer.size(),
        renderer.gpu.transient_count,
    ));
}

pub(super) fn debug(renderer: &mut Renderer, mode: crate::client_commands::MeteorDebug) {
    use crate::client_commands::MeteorDebug;
    renderer.gpu.transient_enabled = true;
    renderer.gpu.transient_transmission = mode == MeteorDebug::Occlusion;
    match mode {
        MeteorDebug::Paths => {
            for line in renderer.meteors.path_lines() {
                renderer.application.ui.console.push_line(line);
            }
        }
        MeteorDebug::Budget | MeteorDebug::Light => status(renderer),
        _ => {}
    }
    renderer.application.ui.console.push_line(format!("meteor diagnostic {mode:?}; paths/budget/light print a snapshot; occlusion persists until debug off"));
}

pub(super) fn push(snapshot: &mut SkyDiagnosticsSnapshot, renderer: &Renderer) {
    snapshot.push("transient_light_enabled", renderer.gpu.transient_enabled);
    snapshot.push("transient_lights", renderer.gpu.transient_count);
    snapshot.push(
        "transient_observer_irradiance",
        renderer.gpu.transient_incident,
    );
    snapshot.push(
        "transient_buffer_bytes",
        renderer.gpu.transient_buffer.size(),
    );
    snapshot.push(
        "transient_transmission_debug",
        renderer.gpu.transient_transmission,
    );
    snapshot.push(
        "transient_cloud_records",
        renderer.gpu.cloud_gpu.light_records.len(),
    );
    snapshot.push(
        "transient_cloud_proxy_limit",
        if renderer.gpu.cloud_gpu.standard_lighting && !renderer.gpu.milky_way.meteors.low_quality {
            8
        } else {
            0
        },
    );
    let status = renderer.gpu.meteors_status;
    let gpu = &renderer.gpu.milky_way.meteors;
    snapshot.push(
        "meteors_optical_heads",
        if renderer.gpu.transient_enabled {
            gpu.optical_heads
        } else {
            0
        },
    );
    snapshot.push(
        "meteors_optics",
        if renderer.gpu.atmosphere_gpu.is_active() {
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
        renderer.gpu.cloud_gpu.light_cache.scans,
    );
    snapshot.push(
        "meteor_cloud_cache_reuse",
        renderer.gpu.cloud_gpu.light_cache.reused,
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

pub(super) fn optical_status(renderer: &Renderer) -> String {
    format!(
        "meteors optics: {}, selected sources {}, halo heads {}, light {}",
        if renderer.gpu.atmosphere_gpu.is_active() {
            "bounded atmospheric halo"
        } else {
            "analytic narrow fallback"
        },
        renderer.gpu.transient_count,
        if renderer.gpu.transient_enabled {
            renderer.gpu.milky_way.meteors.optical_heads
        } else {
            0
        },
        if renderer.gpu.transient_enabled {
            "on"
        } else {
            "off"
        }
    )
}
