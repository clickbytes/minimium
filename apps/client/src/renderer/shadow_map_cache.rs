use super::{geometry::Vertex, shadows::ShadowInvalidReason, GpuResources};
const MAX_MAP_VERTEX_BYTES: u64 = 2 * 1024 * 1024;

impl GpuResources {
    pub(super) fn update_shadow_map_vertices(
        &mut self,
        map: &minimium_core::world::WorldMap,
        center: [f64; 3],
    ) {
        let fingerprint = map.fingerprint();
        if self.shadow_map_instance == Some(self.active_world_instance)
            && self.shadow_map_fingerprint == Some(fingerprint)
            && self.shadow_map_center == Some(center)
        {
            return;
        }
        let geometry = super::shadow_casters::local_map_geometry(
            map,
            center,
            f64::from(self.shadow_gpu.settings.caster_radius_m),
        );
        let vertices = super::geometry::sector_geometry_relative(&geometry, self.render_origin);
        let bytes = vertices.len() as u64 * std::mem::size_of::<Vertex>() as u64;
        self.shadow_map_vertex_count = 0;
        if bytes > MAX_MAP_VERTEX_BYTES {
            self.shadow_state
                .invalidate(ShadowInvalidReason::BudgetExceeded);
            self.shadow_map_center = None;
            return;
        }
        if !vertices.is_empty() {
            if self.shadow_map_buffer.is_none() {
                let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("local-shadow-map-casters"),
                    size: MAX_MAP_VERTEX_BYTES,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.fixed_gpu_bytes = self.fixed_gpu_bytes.saturating_add(buffer.size());
                self.shadow_map_buffer = Some(buffer);
            }
            let Some(buffer) = &self.shadow_map_buffer else {
                self.shadow_state
                    .invalidate(ShadowInvalidReason::IncompleteCasters);
                return;
            };
            self.queue
                .write_buffer(buffer, 0, bytemuck::cast_slice(&vertices));
            self.shadow_map_vertex_count = vertices.len() as u32;
        }
        self.shadow_map_instance = Some(self.active_world_instance);
        self.shadow_map_fingerprint = Some(fingerprint);
        self.shadow_map_center = Some(center);
    }

    pub(super) fn release_shadow_map_casters(&mut self) {
        if let Some(buffer) = self.shadow_map_buffer.take() {
            self.fixed_gpu_bytes = self.fixed_gpu_bytes.saturating_sub(buffer.size());
        }
        self.shadow_map_instance = None;
        self.shadow_map_fingerprint = None;
        self.shadow_map_center = None;
        self.shadow_map_vertex_count = 0;
    }
}
