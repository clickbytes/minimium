use super::{
    environment::{EnvironmentFrame, EnvironmentFrameStatus},
    shadows::{self, ShadowInvalidReason, ShadowUniform},
    GpuResources,
};
use bytemuck::Zeroable;
use minimium_renderer::{geometry::Vertex, ShadowSettings};

impl GpuResources {
    pub(super) fn set_shadow_settings(
        &mut self,
        settings: ShadowSettings,
    ) -> Result<(), &'static str> {
        let previous_bytes = self.shadow_gpu.gpu_bytes();
        self.shadow_gpu.reconfigure(&self.device, settings)?;
        let current_bytes = self.shadow_gpu.gpu_bytes();
        self.fixed_gpu_bytes = self
            .fixed_gpu_bytes
            .saturating_sub(previous_bytes)
            .saturating_add(current_bytes);
        if !settings.enabled {
            self.release_shadow_map_casters();
            self.fixed_gpu_bytes = self
                .fixed_gpu_bytes
                .saturating_sub(self.shadow_gpu.release_player_casters());
            self.shadow_remote_complete = false;
            self.shadow_remote_dirty = true;
            self.shadow_state.invalidate(ShadowInvalidReason::Disabled);
        }
        Ok(())
    }

    pub(super) fn update_remote_shadow_vertices(&mut self, vertices: &[Vertex]) {
        if !self.shadow_gpu.settings.enabled {
            self.shadow_gpu.clear_player_casters();
            self.shadow_remote_complete = false;
            self.shadow_remote_dirty = true;
            return;
        }
        self.shadow_remote_complete = true;
        self.shadow_remote_dirty = false;
        let size = (super::MAX_VISIBLE_PLAYERS * 36 * std::mem::size_of::<Vertex>()) as u64;
        match self
            .shadow_gpu
            .upload_player_casters(&self.device, &self.queue, vertices, size)
        {
            Ok(allocated_bytes) => {
                self.fixed_gpu_bytes = self.fixed_gpu_bytes.saturating_add(allocated_bytes);
            }
            Err(()) => {
                self.shadow_remote_complete = false;
                self.shadow_remote_dirty = true;
                self.shadow_state
                    .invalidate(ShadowInvalidReason::BudgetExceeded);
            }
        }
    }

    pub(super) fn prepare_shadow_frame(
        &mut self,
        environment: &EnvironmentFrame,
        map: Option<&minimium_world::world::WorldMap>,
        player_position_m: [f64; 3],
    ) {
        self.shadow_state.caster_keys.clear();
        self.shadow_state.valid = false;
        self.shadow_state.source = None;
        self.shadow_state.invalid_reason = None;
        if !self.shadow_gpu.settings.enabled {
            self.shadow_state.invalidate(ShadowInvalidReason::Disabled);
            self.shadow_gpu
                .write_uniform(&self.queue, &ShadowUniform::zeroed());
            return;
        }
        if environment.status != EnvironmentFrameStatus::Available
            || !self.scene_visible
            || (self.overview_active && !self.overview_planet)
        {
            self.shadow_state
                .invalidate(ShadowInvalidReason::InvalidFrame);
            self.shadow_gpu
                .write_uniform(&self.queue, &ShadowUniform::zeroed());
            return;
        }
        let Some(context) = environment.context else {
            self.shadow_state
                .invalidate(ShadowInvalidReason::StaleContext);
            self.shadow_gpu
                .write_uniform(&self.queue, &ShadowUniform::zeroed());
            return;
        };
        if context.instance() != self.active_world_instance
            || self
                .world_geometry
                .versions()
                .get(&self.active_world_instance)
                .is_none_or(|(_, revision)| *revision != context.content_revision())
        {
            self.shadow_state
                .invalidate(ShadowInvalidReason::StaleContext);
            self.shadow_gpu
                .write_uniform(&self.queue, &ShadowUniform::zeroed());
            return;
        }
        let map = match map {
            Some(map) => map,
            None => {
                self.shadow_state
                    .invalidate(ShadowInvalidReason::StaleContext);
                self.shadow_gpu
                    .write_uniform(&self.queue, &ShadowUniform::zeroed());
                return;
            }
        };
        let ground_distance = self
            .active_sector_keys
            .iter()
            .filter_map(|(key, lod)| {
                self.world_geometry
                    .bounds()
                    .get(&(self.active_world_instance, *key, *lod))
            })
            .map(|bounds| bounds.distance_to(environment.render_view.camera_eye_world_m))
            .fold(f64::INFINITY, f64::min);
        let Some(region) = minimium_renderer::shadow_view::ShadowRegion::for_view(
            environment.render_view,
            player_position_m,
            ground_distance,
            self.shadow_gpu.settings,
            self.shadow_state.receiver_radius_m,
        ) else {
            self.shadow_state
                .invalidate(ShadowInvalidReason::IncompleteCasters);
            self.shadow_gpu
                .write_uniform(&self.queue, &ShadowUniform::zeroed());
            return;
        };
        let source = shadows::select_source(
            environment,
            self.shadow_state.previous_source,
            region.center_world,
            self.atmosphere_gpu.is_active(),
        );
        let Some(source) = source else {
            self.shadow_state.previous_source = None;
            self.shadow_state.invalidate(ShadowInvalidReason::NoSource);
            self.shadow_gpu
                .write_uniform(&self.queue, &ShadowUniform::zeroed());
            return;
        };
        self.update_shadow_map_vertices(map, region.center_world, region.settings.caster_radius_m);
        if self.shadow_state.invalid_reason == Some(ShadowInvalidReason::BudgetExceeded) {
            self.shadow_gpu
                .write_uniform(&self.queue, &ShadowUniform::zeroed());
            return;
        }
        if !self.overview_active && !self.shadow_remote_complete {
            self.shadow_state
                .invalidate(ShadowInvalidReason::IncompleteCasters);
            self.shadow_gpu
                .write_uniform(&self.queue, &ShadowUniform::zeroed());
            return;
        }
        if self.active_sector_keys.is_empty() {
            self.shadow_state
                .invalidate(ShadowInvalidReason::IncompleteCasters);
            self.shadow_gpu
                .write_uniform(&self.queue, &ShadowUniform::zeroed());
            return;
        }
        let radius = f64::from(region.settings.caster_radius_m);
        for (key, lod) in &self.active_sector_keys {
            let catalog_key = (self.active_world_instance, *key, *lod);
            if !self.world_geometry.catalog().contains_key(&catalog_key) {
                self.shadow_state
                    .invalidate(ShadowInvalidReason::IncompleteCasters);
                self.shadow_gpu
                    .write_uniform(&self.queue, &ShadowUniform::zeroed());
                return;
            }
            let Some(bounds) = self.world_geometry.bounds().get(&catalog_key) else {
                self.shadow_state
                    .invalidate(ShadowInvalidReason::IncompleteCasters);
                self.shadow_gpu
                    .write_uniform(&self.queue, &ShadowUniform::zeroed());
                return;
            };
            if bounds.intersects_sphere(region.center_world, radius) {
                self.shadow_state.caster_keys.push((*key, *lod));
            }
        }
        if self.shadow_state.caster_keys.is_empty() {
            self.shadow_state
                .invalidate(ShadowInvalidReason::IncompleteCasters);
            self.shadow_gpu
                .write_uniform(&self.queue, &ShadowUniform::zeroed());
            return;
        }
        let Some(direction) = source.direction(environment, self.atmosphere_gpu.is_active()) else {
            self.shadow_state
                .invalidate(ShadowInvalidReason::InvalidFrame);
            self.shadow_gpu
                .write_uniform(&self.queue, &ShadowUniform::zeroed());
            return;
        };
        let Some(uniform) = minimium_renderer::shadow_uniform::prepare_at_origin(
            direction,
            region.center_world,
            self.render_origin.map(f64::from),
            region.settings,
            source.id(),
        ) else {
            self.shadow_state
                .invalidate(ShadowInvalidReason::InvalidFrame);
            self.shadow_gpu
                .write_uniform(&self.queue, &ShadowUniform::zeroed());
            return;
        };
        self.shadow_state.previous_source = Some(source);
        self.shadow_state.source = Some(source);
        self.shadow_state.valid = true;
        self.shadow_state.receiver_radius_m = region.settings.receiver_radius_m;
        self.shadow_state.texel_m =
            region.settings.receiver_radius_m * 2.02 / region.settings.resolution.max(1) as f32;
        self.shadow_state.invalid_reason = None;
        self.shadow_gpu.write_uniform(&self.queue, &uniform);
    }
}
