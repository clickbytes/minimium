use super::*;
impl Renderer {
    pub(super) fn resolved_map_for_context(
        &mut self,
        context: minimium_protocol::WorldContext,
        manifest: WorldManifest,
    ) -> Option<std::sync::Arc<WorldMap>> {
        let Some(content) = self
            .application
            .connection
            .session()
            .and_then(|session| session.resolved_world_content_for_context(context))
        else {
            return None;
        };
        if content.manifest != manifest {
            return None;
        }
        Some(content.map.clone())
    }

    pub(super) fn install_world(
        &mut self,
        owner: super::super::sector_scene::SceneInterestOwner,
        context: minimium_protocol::WorldContext,
        fingerprint: u64,
        destination: Option<[f32; 3]>,
        manifest: WorldManifest,
    ) -> bool {
        let instance = context.instance();
        let Some(map) = self
            .application
            .connection
            .session()
            .and_then(|session| session.resolved_world_content_for_context(context))
            .map(|content| content.map.clone())
        else {
            return false;
        };
        if map.fingerprint() != fingerprint {
            return false;
        }
        let target_position = destination.unwrap_or_else(|| map.spawn());
        let water = self.water_presentation_for(context);
        let Some(terrain_inputs) = self
            .application
            .connection
            .session()
            .and_then(|session| session.terrain_inputs_for_context(context))
        else {
            return false;
        };
        let scene = match self.sector_cache.ensure_scene(
            owner,
            instance,
            manifest,
            target_position,
            minimium_renderer::terrain_view::TerrainView::new(
                target_position,
                self.fov_degrees.to_radians(),
                self.gpu.size.height,
            ),
            self.gpu.render_origin,
            self.gpu.world_geometry.origin_generation(),
            &self.application.world_catalog,
            &self.application.game.world_manifests,
            Some((map.as_ref(), manifest)),
            terrain_inputs,
            water,
        ) {
            Ok(scene) => scene,
            Err(error) => {
                self.application
                    .connection
                    .record_diagnostic_failure(error.code(), &error.to_string());
                return false;
            }
        };
        let gpu_ready = scene.as_ref().is_some_and(|scene| {
            self.gpu.cache_sector_scene(instance, scene)
                && self.gpu.activate_sector_scene(instance, scene, fingerprint)
        });
        self.application.set_loading(!gpu_ready);
        self.gpu.scene_visible = gpu_ready;
        let previous_instance = self.application.game.current_instance;
        self.application.game.gameplay.collision_index = scene.as_ref().map_or_else(
            || minimium_simulation::CollisionIndex::from_geometry(Vec::new(), Vec::new()),
            |s| s.collision.clone(),
        );
        self.application.game.gameplay.collision_revision = scene.as_ref().map_or_else(
            || {
                self.application
                    .game
                    .gameplay
                    .collision_revision
                    .wrapping_add(1)
                    .max(1)
            },
            |s| s.collision_revision,
        );
        self.application.game.gameplay.prediction.character = CharacterState::at_spawn_with_gravity(
            target_position,
            super::camera::gravity_for_manifest(manifest),
        );
        self.application.game.gameplay.prediction.position_mm = manifest.planet().map(|_| {
            if target_position == map.spawn() {
                map.spawn_position_mm().unwrap_or_else(|| {
                    minimium_world::world::Position3Mm(
                        target_position.map(|value| (f64::from(value) * 1000.0).round() as i64),
                    )
                })
            } else {
                minimium_world::world::Position3Mm(
                    target_position.map(|value| (f64::from(value) * 1000.0).round() as i64),
                )
            }
        });
        let previous_context = self.application.game.session_context();
        self.application.game.gameplay.sectors_ready = scene.is_some();
        self.last_render_position = if gpu_ready {
            target_position
        } else {
            [f32::INFINITY; 3]
        };
        self.application.game.gameplay.prediction.reset();
        self.reset_remote_gameplay();
        self.input.yaw = -std::f32::consts::FRAC_PI_2;
        self.input.pitch = 0.0;
        self.application.input.clear_held();
        self.input.jump_pending = false;
        self.application.activate_world(instance);
        if let Some(source) = previous_context {
            self.sector_cache.promote_interest(
                owner,
                super::super::sector_scene::SceneInterestOwner::Active(context),
            );
            self.sector_cache.release_interest(
                super::super::sector_scene::SceneInterestOwner::Active(source),
            );
        }
        if previous_instance != instance {
            self.release_world_sectors(previous_instance);
        }
        self.metrics.frame_stats.set_gpu_bytes(
            self.gpu.fixed_gpu_bytes
                + u64::from(self.gpu.size.width.max(1))
                    * u64::from(self.gpu.size.height.max(1))
                    * 4,
        );
        true
    }
}

impl Renderer {
    pub(super) fn apply_altitude_position(&mut self, position: minimium_world::world::Position3Mm) {
        self.application.game.gameplay.prediction.position_mm = Some(position);
        self.application.game.gameplay.prediction.character.position =
            position.0.map(|v| v as f32 / 1000.0);
        self.gpu.set_render_origin(
            self.application
                .game
                .gameplay
                .prediction
                .character
                .position
                .map(|v| (v / 4096.0).round() * 4096.0),
        );
        self.shadow_state
            .invalidate(super::super::shadows::ShadowInvalidReason::StaleContext);
        self.application.game.gameplay.prediction.character.velocity = [0.0; 3];
        self.application.game.gameplay.prediction.character.grounded = false;
        self.application.game.gameplay.prediction.reset();
        self.last_render_position = [f32::INFINITY; 3];
        self.application.game.gameplay.sectors_ready = false;
        self.application.set_loading(true);
        self.gpu.scene_visible = false;
        self.application.input.clear_held();
        self.application.status =
            crate::application::ClientStatus::Status(minimium_localization::UiStatus::EarthLoading);
    }
}
