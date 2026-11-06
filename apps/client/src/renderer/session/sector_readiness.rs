use super::*;

impl Renderer {
    pub(in crate::renderer) fn refresh_active_sector_scene(&mut self) -> bool {
        let Some(manifest) = self
            .game
            .world_manifests
            .get(&self.game.current_instance)
            .copied()
        else {
            self.game.gameplay.sectors_ready = false;
            return false;
        };
        let view = self.terrain_view();
        let Some(scene) = self.sector_cache.ensure_scene(
            self.game.current_instance,
            manifest,
            self.game.gameplay.prediction.character.position,
            view,
            self.gpu.render_origin,
            &self.world_catalog,
            &self.game.world_manifests,
            self.connection
                .session()
                .and_then(|session| session.prepared_geography()),
        ) else {
            self.game.gameplay.sectors_ready = false;
            return false;
        };
        if !self
            .gpu
            .cache_sector_scene(self.game.current_instance, &scene)
            || !self
                .gpu
                .activate_sector_scene(self.game.current_instance, &scene)
        {
            self.game.gameplay.collision_index = scene.collision;
            self.last_render_position = [f32::INFINITY; 3];
            self.game.gameplay.sectors_ready = true;
            return true;
        }
        self.game.gameplay.collision_index = scene.collision;
        self.active_sector_keys.clone_from(&scene.keys);
        self.last_render_position = scene
            .prepared
            .as_ref()
            .map_or(self.game.gameplay.prediction.character.position, |p| {
                p.position()
            });
        self.game.gameplay.sectors_ready = true;
        true
    }

    pub(in crate::renderer) fn advance_pending_transitions(&mut self) {
        let pending: Vec<_> = self
            .pending_sector_transitions
            .iter()
            .filter(|(_, pending)| !pending.ready_sent)
            .map(|(id, pending)| (*id, *pending))
            .collect();
        for (transition_id, pending) in pending {
            let instance = pending.context.instance();
            let Some(scene) = self.sector_cache.ensure_scene(
                instance,
                pending.manifest,
                pending.destination,
                minimium_renderer::terrain_view::TerrainView::new(
                    pending.destination,
                    self.fov_degrees.to_radians(),
                    self.gpu.size.height,
                ),
                self.gpu.render_origin,
                &self.world_catalog,
                &self.game.world_manifests,
                self.connection
                    .session()
                    .and_then(|session| session.prepared_geography()),
            ) else {
                continue;
            };
            if !self.gpu.cache_sector_scene(instance, &scene) {
                continue;
            }
            let Some(session) = self.connection.session_mut() else {
                continue;
            };
            if session
                .queue_transition_ready(transition_id, pending.context)
                .is_ok()
            {
                if let Some(pending) = self.pending_sector_transitions.get_mut(&transition_id) {
                    // Keep the target until Commit/Cancel so cancellation can
                    // release its pins and GPU buffers after Ready was queued.
                    pending.ready_sent = true;
                }
            }
        }
    }

    pub(in crate::renderer) fn clear_pending_sector_transitions(&mut self) {
        for (_, pending) in std::mem::take(&mut self.pending_sector_transitions) {
            self.release_prepared_target(pending.context.instance());
        }
    }

    pub(super) fn release_prepared_target(
        &mut self,
        instance: minimium_world::world::WorldInstanceId,
    ) {
        if instance != self.game.current_instance {
            self.sector_cache.release_interest(instance);
            self.gpu.release_world_sectors(instance);
        }
    }
}
