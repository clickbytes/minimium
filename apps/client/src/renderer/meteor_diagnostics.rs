use super::DesktopHost;
use crate::environment::SkyDiagnosticsSnapshot;

pub(super) fn status(renderer: &mut DesktopHost) -> Vec<String> {
    let mut output = Vec::new();
    use crate::client_commands::{ClientPropertyCommand as Command, ClientPropertyKey as Key};
    for key in [
        Key::MeteorEnabled,
        Key::MeteorRate,
        Key::MeteorQuality,
        Key::MeteorSeed,
        Key::MeteorAudio,
    ] {
        let line = renderer.apply_property(Command::Get(key));
        output.push(line);
    }
    let gpu = &renderer.renderer.milky_way.meteors;
    output.push(format!("meteor upload CPU {} us; culled segments {}; quality-dropped cores {}; cloud scans {} / reused {}; proxy cache {} bytes", gpu.prepare_us, gpu.culled_segments, gpu.dropped_cores, renderer.renderer.cloud_gpu.light_cache.scans, renderer.renderer.cloud_gpu.light_cache.reused, std::mem::size_of::<minimium_renderer::transient_cloud_cache::CloudLightCache>()));
    output.push(format!(
        "meteor GPU: cores {}, segments {}, dust {}, culled low {}, upload {} bytes, persistent {} bytes, lights {}",
        renderer.renderer.meteors_status.core_count(), gpu.count, gpu.dust_segments,
        gpu.rejected_low, gpu.upload_bytes,
        minimium_renderer::meteors_gpu::MeteorGpu::BYTES + renderer.renderer.transient_buffer.size(),
        renderer.renderer.transient_count,
    ));
    output
}

pub(super) fn debug(
    renderer: &mut DesktopHost,
    mode: crate::client_commands::MeteorDebug,
) -> Vec<String> {
    let mut output = Vec::new();
    use crate::client_commands::MeteorDebug;
    renderer.renderer.transient_enabled = true;
    renderer.renderer.transient_transmission = mode == MeteorDebug::Occlusion;
    match mode {
        MeteorDebug::Paths => {
            for line in renderer.meteors.path_lines() {
                output.push(line);
            }
        }
        MeteorDebug::Budget | MeteorDebug::Light => output.extend(status(renderer)),
        _ => {}
    }
    output.push(format!("meteor diagnostic {mode:?}; paths/budget/light print a snapshot; occlusion persists until debug off"));
    output
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

pub(super) fn optical_status(renderer: &DesktopHost) -> String {
    format!(
        "meteors optics: {}, selected sources {}, halo heads {}, light {}",
        if renderer.renderer.atmosphere_gpu.is_active() {
            "bounded atmospheric halo"
        } else {
            "analytic narrow fallback"
        },
        renderer.renderer.transient_count,
        if renderer.renderer.transient_enabled {
            renderer.renderer.milky_way.meteors.optical_heads
        } else {
            0
        },
        if renderer.renderer.transient_enabled {
            "on"
        } else {
            "off"
        }
    )
}
