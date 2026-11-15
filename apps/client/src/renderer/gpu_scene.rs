use super::{sector_scene::ClientSectorScene, GpuResources, Renderer};
use minimium_renderer::geometry::Vertex;

impl GpuResources {
    pub(super) fn cache_sector_scene(
        &mut self,
        instance: minimium_world::world::WorldInstanceId,
        scene: &ClientSectorScene,
    ) -> bool {
        self.shadow_publish_ready = false;
        if scene.origin != self.render_origin
            || scene.origin_generation != self.world_geometry.origin_generation()
        {
            return false;
        }
        let mut result = if let Some(prepared) = &scene.prepared {
            let active = self
                .world_geometry
                .publication()
                .filter(|publication| publication.stamp.instance == instance)
                .map_or_else(Vec::new, |publication| publication.keys.clone());
            self.world_geometry.cache_prepared(
                &self.device,
                instance,
                scene.generation,
                scene.content_revision,
                &prepared.patches,
                &active,
                scene.origin_generation,
            )
        } else if scene.render_sectors.is_empty() {
            return false;
        } else {
            self.world_geometry.cache_scene(
                &self.device,
                instance,
                scene.generation,
                scene.content_revision,
                &scene.render_sectors,
                self.render_origin,
                None,
                scene.origin_generation,
            )
        };
        if let Some(prepared) = &scene.prepared {
            let water = self.world_geometry.cache_local_water(
                &self.device,
                instance,
                prepared.water_vertices.clone(),
                scene.origin_generation,
            );
            result.ready &= water.ready;
            result.allocated_bytes += water.allocated_bytes;
            result.released_bytes += water.released_bytes;
        }
        self.fixed_gpu_bytes = self
            .fixed_gpu_bytes
            .saturating_sub(result.released_bytes)
            .saturating_add(result.allocated_bytes);
        result.ready
    }

    pub(super) fn activate_sector_scene(
        &mut self,
        instance: minimium_world::world::WorldInstanceId,
        scene: &ClientSectorScene,
        fingerprint: u64,
    ) -> bool {
        self.shadow_publish_ready = false;
        if scene.origin_generation != self.world_geometry.origin_generation() {
            return false;
        }
        let keys = scene
            .render_sectors
            .iter()
            .map(|(key, lod, _)| (*key, *lod))
            .collect::<Vec<_>>();
        let ready = self
            .world_geometry
            .publish_scene(instance, fingerprint, &keys);
        self.fixed_gpu_bytes = self
            .fixed_gpu_bytes
            .saturating_sub(self.world_geometry.take_released_publication_bytes());
        ready
    }

    pub(super) fn release_world_sectors(
        &mut self,
        instance: minimium_world::world::WorldInstanceId,
    ) {
        self.shadow_publish_ready = false;
        self.fixed_gpu_bytes = self
            .fixed_gpu_bytes
            .saturating_sub(self.world_geometry.release_world(instance));
        if self
            .shadow_map_key
            .is_some_and(|key| key.instance == instance)
        {
            self.release_shadow_map_casters();
        }
    }

    pub(super) fn update_remote_vertices(&mut self, vertices: &[Vertex]) {
        self.remote_vertices.upload(&self.queue, vertices);
    }

    pub(super) fn set_render_origin(&mut self, origin: [f32; 3]) -> bool {
        if self.render_origin == origin {
            return false;
        }
        let delta = std::array::from_fn(|axis| self.render_origin[axis] - origin[axis]);
        self.render_origin = origin;
        self.shadow_publish_ready = false;
        if !self.world_geometry.rebase(&self.device, &self.queue, delta) {
            self.fixed_gpu_bytes = self
                .fixed_gpu_bytes
                .saturating_sub(self.world_geometry.clear());
        }
        self.shadow_gpu.clear_player_casters();
        self.shadow_remote_complete = false;
        self.shadow_remote_stamp = None;
        self.shadow_remote_dirty = true;
        self.release_shadow_map_casters();
        true
    }
}

impl Renderer {
    pub(super) fn release_world_sectors(
        &mut self,
        instance: minimium_world::world::WorldInstanceId,
    ) {
        self.shadow_map_preparation.release_world(instance);
        self.sector_cache.remove_world_data(instance);
        self.gpu.release_world_sectors(instance);
    }
}
