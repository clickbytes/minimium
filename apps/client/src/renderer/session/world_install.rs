use super::*;
impl Renderer {
    pub(super) fn install_world(
        &mut self,
        instance: minimium_world::world::WorldInstanceId,
        fingerprint: u64,
        destination: Option<[f32; 3]>,
        manifest: WorldManifest,
    ) -> bool {
        let Some(map) = self.world_catalog.get(&instance) else {
            return false;
        };
        if map.fingerprint() != fingerprint {
            return false;
        }
        if self.game.world_manifests.get(&instance) != Some(&manifest) {
            return false;
        }
        let target_position = destination.unwrap_or_else(|| map.spawn());
        let scene = self.sector_cache.ensure_scene(
            instance,
            manifest,
            target_position,
            minimium_renderer::terrain_view::TerrainView::new(
                target_position,
                self.fov_degrees.to_radians(),
                self.gpu.size.height,
            ),
            self.gpu.render_origin,
            &self.world_catalog,
            &self.game.world_manifests,
            self.connection
                .session()
                .and_then(|session| session.prepared_geography()),
        );
        let gpu_ready = scene.as_ref().is_some_and(|scene| {
            self.gpu.cache_sector_scene(instance, scene)
                && self.gpu.activate_sector_scene(instance, scene)
        });
        self.teleport_loading = !gpu_ready;
        self.gpu.scene_visible = gpu_ready;
        let previous_instance = self.game.current_instance;
        self.game.collision_index = scene.as_ref().map_or_else(
            || minimium_simulation::CollisionIndex::from_geometry(Vec::new(), Vec::new()),
            |s| s.collision.clone(),
        );
        self.game.character = CharacterState::at_spawn_with_gravity(
            target_position,
            super::camera::gravity_for_manifest(manifest),
        );
        self.game.position_mm = manifest.planet().map(|_| {
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
        self.game.sectors_ready = scene.is_some();
        self.active_sector_keys = scene.as_ref().map_or_else(Vec::new, |s| s.keys.clone());
        self.last_render_position = if gpu_ready {
            target_position
        } else {
            [f32::INFINITY; 3]
        };
        self.game.prediction.reset_time();
        self.reset_remote_gameplay();
        self.input.yaw = -std::f32::consts::FRAC_PI_2;
        self.input.pitch = 0.0;
        self.input.keys.clear();
        self.input.jump_pending = false;
        self.input.last_sent_direction = [0.0; 2];
        self.input.input_keepalive = 0.0;
        self.game.current_instance = instance;
        self.pending_sector_transitions
            .retain(|_, pending| pending.context.instance() != instance);
        if previous_instance != instance {
            self.sector_cache.release_interest(previous_instance);
            self.gpu.release_world_sectors(previous_instance);
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
        self.game.position_mm = Some(position);
        self.game.character.position = position.0.map(|v| v as f32 / 1000.0);
        self.gpu.set_render_origin(
            self.game
                .character
                .position
                .map(|v| (v / 4096.0).round() * 4096.0),
        );
        self.game.character.velocity = [0.0; 3];
        self.game.character.grounded = false;
        self.game.prediction.reset_time();
        self.last_render_position = [f32::INFINITY; 3];
        self.game.sectors_ready = false;
        self.teleport_loading = true;
        self.gpu.scene_visible = false;
        self.input.keys.clear();
        self.input.last_sent_direction = [0.0; 2];
        self.connection_status = "set_altitude: loading terrain...".into();
    }
}
