use super::{
    environment::{EnvironmentFrame, EnvironmentFrameStatus, EnvironmentViewKind},
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
            || environment.render_view.kind != EnvironmentViewKind::Surface
            || !self.scene_visible
            || self.overview_active
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
        let source = shadows::select_source(environment, self.shadow_state.previous_source);
        let Some(source) = source else {
            self.shadow_state.previous_source = None;
            self.shadow_state.invalidate(ShadowInvalidReason::NoSource);
            self.shadow_gpu
                .write_uniform(&self.queue, &ShadowUniform::zeroed());
            return;
        };
        self.update_shadow_map_vertices(map, player_position_m);
        if self.shadow_state.invalid_reason == Some(ShadowInvalidReason::BudgetExceeded) {
            self.shadow_gpu
                .write_uniform(&self.queue, &ShadowUniform::zeroed());
            return;
        }
        if !self.shadow_remote_complete {
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
        let radius = f64::from(self.shadow_gpu.settings.caster_radius_m);
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
            if bounds.intersects_sphere(player_position_m, radius) {
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
        let Some(direction) = source.direction(environment) else {
            self.shadow_state
                .invalidate(ShadowInvalidReason::InvalidFrame);
            self.shadow_gpu
                .write_uniform(&self.queue, &ShadowUniform::zeroed());
            return;
        };
        let center_relative = std::array::from_fn(|axis| {
            player_position_m[axis] - f64::from(self.render_origin[axis])
        });
        let Some(uniform) = minimium_renderer::shadow_uniform::prepare(
            direction,
            center_relative,
            self.shadow_gpu.settings,
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
        self.shadow_state.invalid_reason = None;
        self.shadow_gpu.write_uniform(&self.queue, &uniform);
    }
}
