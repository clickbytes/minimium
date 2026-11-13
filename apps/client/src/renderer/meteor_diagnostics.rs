use super::Renderer;
use crate::environment::SkyDiagnosticsSnapshot;

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
    let status = renderer.gpu.meteors_status;
    let gpu = &renderer.gpu.milky_way.meteors;
    snapshot.push("meteors_segments", gpu.count);
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
