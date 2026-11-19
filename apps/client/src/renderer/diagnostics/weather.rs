use super::super::DesktopHost;
use crate::environment::SkyDiagnosticsSnapshot;
pub(super) fn push_shadow_fields(snapshot: &mut SkyDiagnosticsSnapshot, renderer: &DesktopHost) {
    let gpu = &renderer.renderer;
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
    let landscape = renderer.prepared_shadow.landscape_uniform.filter(|_| valid);
    snapshot.push_optional(
        "shadow_landscape_range_m",
        landscape.map(|uniform| uniform.receiver_center_relative_radius[3]),
    );
    snapshot.push_optional(
        "shadow_landscape_sector_count",
        landscape.map(|_| renderer.prepared_shadow.landscape_caster_keys.len()),
    );
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
                super::super::shadow_gpu::ShadowUploadStatus::Ready => "not_rendered",
                super::super::shadow_gpu::ShadowUploadStatus::Pending => "upload_pending",
                super::super::shadow_gpu::ShadowUploadStatus::StaleContext => "stale_context",
                super::super::shadow_gpu::ShadowUploadStatus::Failed => "upload_failed",
            }
        },
    );
}

pub(super) fn push_gpu_fields(snapshot: &mut SkyDiagnosticsSnapshot, renderer: &DesktopHost) {
    let gpu = &renderer.renderer;
    let star_bytes = gpu.stars.buffer.size();
    let uniform_bytes = gpu.environment_buffer.size()
        + gpu.camera_buffer.size()
        + gpu.transient_buffer.size()
        + gpu.shadow_gpu.uniform_bytes();
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
