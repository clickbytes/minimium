use super::{shadows::ShadowInvalidReason, GpuResources};
use minimium_renderer::geometry::Vertex;
const MAX_MAP_VERTEX_BYTES: u64 = 2 * 1024 * 1024;

impl GpuResources {
    pub(super) fn update_shadow_map_vertices(
        &mut self,
        map: &minimium_world::world::WorldMap,
        center: [f64; 3],
    ) {
        let fingerprint = map.fingerprint();
        if self.shadow_map_instance == Some(self.active_world_instance)
            && self.shadow_map_fingerprint == Some(fingerprint)
            && self.shadow_map_center == Some(center)
        {
            return;
        }
        let geometry = minimium_renderer::shadow_geometry::local_map_geometry(
            map,
            center,
            f64::from(self.shadow_gpu.settings.caster_radius_m),
        );
        let vertices =
            minimium_renderer::geometry::sector_geometry_relative(&geometry, self.render_origin);
        let bytes = vertices.len() as u64 * std::mem::size_of::<Vertex>() as u64;
        if bytes > MAX_MAP_VERTEX_BYTES {
            self.shadow_state
                .invalidate(ShadowInvalidReason::BudgetExceeded);
            self.shadow_map_center = None;
            self.shadow_gpu.clear_map_casters();
            return;
        }
        match self.shadow_gpu.upload_map_casters(
            &self.device,
            &self.queue,
            &vertices,
            MAX_MAP_VERTEX_BYTES,
        ) {
            Ok(allocated_bytes) => {
                self.fixed_gpu_bytes = self.fixed_gpu_bytes.saturating_add(allocated_bytes);
            }
            Err(()) => {
                self.shadow_state
                    .invalidate(ShadowInvalidReason::BudgetExceeded);
                self.shadow_map_center = None;
                return;
            }
        }
        self.shadow_map_instance = Some(self.active_world_instance);
        self.shadow_map_fingerprint = Some(fingerprint);
        self.shadow_map_center = Some(center);
    }

    pub(super) fn release_shadow_map_casters(&mut self) {
        self.fixed_gpu_bytes = self
            .fixed_gpu_bytes
            .saturating_sub(self.shadow_gpu.release_map_casters());
        self.shadow_map_instance = None;
        self.shadow_map_fingerprint = None;
        self.shadow_map_center = None;
    }
}
