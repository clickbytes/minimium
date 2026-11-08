use super::*;

impl Renderer {
    pub(in crate::renderer) fn refresh_active_sector_scene(&mut self) -> bool {
        let Some(manifest) = self
            .application
            .game
            .world_manifests
            .get(&self.application.game.current_instance)
            .copied()
        else {
            self.application.game.gameplay.sectors_ready = false;
            return false;
        };
        let view = self.terrain_view();
        let scene = match self.sector_cache.ensure_scene(
            self.application.game.current_instance,
            manifest,
            self.application.game.gameplay.prediction.character.position,
            view,
            self.gpu.render_origin,
            self.gpu.world_geometry.origin_generation(),
            &self.application.world_catalog,
            &self.application.game.world_manifests,
            self.application
                .connection
                .session()
                .and_then(|session| session.prepared_geography()),
        ) {
            Ok(Some(scene)) => scene,
            Ok(None) => {
                self.application.game.gameplay.sectors_ready = false;
                return false;
            }
            Err(error) => {
                self.application
                    .connection
                    .record_diagnostic_failure(error.code(), &error.to_string());
                self.application.game.gameplay.sectors_ready = false;
                return false;
            }
        };
        if !self
            .gpu
            .cache_sector_scene(self.application.game.current_instance, &scene)
            || !self.gpu.activate_sector_scene(
                self.application.game.current_instance,
                &scene,
                self.application
                    .world_catalog
                    .get(&self.application.game.current_instance)
                    .map_or(0, |map| map.fingerprint()),
            )
        {
            self.application.game.gameplay.collision_index = scene.collision;
            self.application.game.gameplay.collision_revision = scene.collision_revision;
            self.last_render_position = [f32::INFINITY; 3];
            self.application.game.gameplay.sectors_ready = true;
            return true;
        }
        self.application.game.gameplay.collision_index = scene.collision;
        self.application.game.gameplay.collision_revision = scene.collision_revision;
        self.last_render_position = scene.prepared.as_ref().map_or(
            self.application.game.gameplay.prediction.character.position,
            |p| p.position(),
        );
        self.application.game.gameplay.sectors_ready = true;
        true
    }

    pub(in crate::renderer) fn advance_pending_transitions(&mut self) {
        let pending = self.application.pending_transition_preparations();
        for (transition_id, pending) in pending {
            let instance = pending.context.instance();
            let scene = match self.sector_cache.ensure_scene(
                instance,
                pending.manifest,
                pending.destination,
                minimium_renderer::terrain_view::TerrainView::new(
                    pending.destination,
                    self.fov_degrees.to_radians(),
                    self.gpu.size.height,
                ),
                self.gpu.render_origin,
                self.gpu.world_geometry.origin_generation(),
                &self.application.world_catalog,
                &self.application.game.world_manifests,
                self.application
                    .connection
                    .session()
                    .and_then(|session| session.prepared_geography()),
            ) {
                Ok(Some(scene)) => scene,
                Ok(None) => continue,
                Err(error) => {
                    self.application
                        .connection
                        .record_diagnostic_failure(error.code(), &error.to_string());
                    if let Some(failed) = self.application.fail_transition(transition_id) {
                        self.release_prepared_target(failed.context.instance());
                    }
                    continue;
                }
            };
            if !self.gpu.cache_sector_scene(instance, &scene) {
                continue;
            }
            let Some(session) = self.application.connection.session_mut() else {
                continue;
            };
            if session
                .queue_transition_ready(transition_id, pending.context)
                .is_ok()
            {
                // Keep the target until Commit/Cancel so cancellation can release its pins.
                self.application.mark_transition_ready(transition_id);
            }
        }
    }

    pub(in crate::renderer) fn clear_pending_sector_transitions(&mut self) {
        for pending in self.application.clear_transitions() {
            self.release_prepared_target(pending.context.instance());
        }
    }

    pub(super) fn release_prepared_target(
        &mut self,
        instance: minimium_world::world::WorldInstanceId,
    ) {
        if instance != self.application.game.current_instance {
            self.sector_cache.release_interest(instance);
            self.release_world_sectors(instance);
        }
    }
}
