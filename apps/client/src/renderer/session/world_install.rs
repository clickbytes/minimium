use super::*;
impl Renderer {
    pub(super) fn load_resolved_catalog(
        &mut self,
        instance: minimium_world::world::WorldInstanceId,
        manifest: WorldManifest,
    ) -> bool {
        let Some(content) = self
            .application
            .connection
            .session()
            .and_then(|session| session.resolved_world_content(instance))
        else {
            return false;
        };
        if content.manifest != manifest {
            return false;
        }
        if self.application.game.world_manifests.get(&instance) != Some(&manifest) {
            if let Err(error) = self.sector_cache.reset() {
                self.application
                    .connection
                    .record_diagnostic_failure(error.code(), &error.to_string());
                return false;
            }
            self.release_world_sectors(instance);
        }
        self.application
            .install_world_content(instance, content.manifest, (*content.map).clone());
        true
    }

    pub(super) fn install_world(
        &mut self,
        instance: minimium_world::world::WorldInstanceId,
        fingerprint: u64,
        destination: Option<[f32; 3]>,
        manifest: WorldManifest,
    ) -> bool {
        let Some(map) = self.application.world_catalog.get(&instance).cloned() else {
            return false;
        };
        if map.fingerprint() != fingerprint {
            return false;
        }
        if self.application.game.world_manifests.get(&instance) != Some(&manifest) {
            return false;
        }
        let target_position = destination.unwrap_or_else(|| map.spawn());
        let scene = match self.sector_cache.ensure_scene(
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
            self.application
                .connection
                .session()
                .and_then(|session| session.prepared_geography()),
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
        if previous_instance != instance {
            self.sector_cache.release_interest(previous_instance);
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
