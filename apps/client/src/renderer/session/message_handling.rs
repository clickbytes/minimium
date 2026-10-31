use super::*;

impl Renderer {
    pub(super) fn handle_server_message(&mut self, message: ServerMessage) {
        match message {
            ServerMessage::Welcome {
                id,
                map_fingerprint,
                context,
                health,
                position,
                position_mm,
                manifest,
                ..
            } => {
                if self.settings.require_earth
                    && manifest.planet().is_none_or(|planet| planet.mode != 1)
                {
                    self.fail_connection("serwer nie udostępnia manifestu Earth");
                    return;
                }
                if !self.load_generated_catalog(context.instance(), manifest) {
                    self.fail_connection("server generation manifest is not supported");
                    return;
                }
                if !self.install_world(
                    context.instance(),
                    map_fingerprint,
                    Some(position),
                    manifest,
                ) {
                    self.fail_connection("saved world instance is not available on this client");
                    return;
                }
                self.game.session_context = Some(context);
                self.game.health = health;
                self.gpu.scene_visible = true;
                self.connection_status = if self.settings.require_earth {
                    "Earth: połączono".into()
                } else {
                    "Połączono".into()
                };
                self.game.position_mm = manifest.planet().map(|_| position_mm);
                if self.ui.console.open {
                    self.ui
                        .console
                        .push_line(Text::Connected { id, health }.to_string());
                }
            }
            ServerMessage::Pong => {
                if self.ui.console.open {
                    self.ui.console.push_line(Text::Pong.to_string());
                }
            }
            ServerMessage::MapTeleportResult { position_mm } => {
                self.map_teleport_pending = false;
                if let Some(position_mm) = position_mm {
                    self.game.position_mm = Some(position_mm);
                    self.game.character.position =
                        position_mm.0.map(|coordinate| coordinate as f32 / 1000.0);
                    let origin = self
                        .game
                        .character
                        .position
                        .map(|coordinate| (coordinate / 256.0).round() * 256.0);
                    self.gpu.set_render_origin(origin);
                    // Preserve world weather, invalidate only the previous near-cloud cut.
                    self.gpu.weather_gpu.instances.invalidate_view();
                    self.game.character.velocity = [0.0; 3];
                    self.game.character.grounded = false;
                    self.game.physics_accumulator = 0.0;
                    self.game.sectors_ready = false;
                    self.teleport_loading = true;
                    self.gpu.scene_visible = false;
                    self.last_render_position = [f32::INFINITY; 3];
                    self.input.keys.clear();
                    self.input.last_sent_direction = [0.0; 2];
                    self.connection_status = "Earth: wczytywanie terenu...".into();
                    self.ui.console.push_line("map_telepo: destination reached");
                } else {
                    self.connection_status = "Earth: wybierz punkt na lądzie".into();
                    self.ui
                        .console
                        .push_error("map_telepo: destination unavailable; choose land");
                    if !self.overview_open && self.gpu.scene_visible {
                        self.toggle_overview();
                        self.map_teleport_armed = true;
                    }
                }
            }
            ServerMessage::Accepted if self.ui.console.open => {
                self.ui.console.push_line(Text::Accepted.to_string());
            }
            ServerMessage::Error(code) => {
                self.ui
                    .console
                    .push_error(Text::ServerError { code }.to_string());
            }
            ServerMessage::ShotResult {
                target_id,
                remaining_health,
                killed,
            } => {
                if target_id.is_some() {
                    self.game.hit_confirmation_until =
                        Some(Instant::now() + Duration::from_millis(180));
                }
                if self.ui.console.open {
                    match (target_id, remaining_health, killed) {
                        (Some(target_id), Some(health), true) => self
                            .ui
                            .console
                            .push_line(Text::TargetRespawned { target_id, health }.to_string()),
                        (Some(target_id), Some(health), false) => self
                            .ui
                            .console
                            .push_line(Text::Hit { target_id, health }.to_string()),
                        _ => self.ui.console.push_line(Text::ShotMissed.to_string()),
                    }
                }
            }
            ServerMessage::HealthUpdate(health) => {
                self.game.health = health;
                if self.ui.console.open {
                    self.ui
                        .console
                        .push_line(Text::Health { health }.to_string());
                }
            }
            ServerMessage::Chat {
                sender_id,
                sender_name,
                text,
            } => self.ui.console.push_chat_line(
                &Text::Chat {
                    sender_name,
                    sender_id,
                    text,
                }
                .to_string(),
            ),
            ServerMessage::Bye => {
                if self.ui.console.open {
                    self.ui.console.push_line(Text::ServerClosed.to_string());
                }
                self.clear_connection();
            }
            ServerMessage::Snapshot {
                server_time_ms,
                full_snapshot,
                self_position,
                self_position_mm,
                self_velocity,
                self_grounded,
                players,
                removed_players,
                ..
            } => {
                let snapshot = SnapshotUpdate {
                    server_time_ms,
                    full_snapshot,
                    self_position,
                    self_position_mm,
                    self_velocity,
                    self_grounded,
                    players: *players,
                    removed_players,
                };
                if !self.game.apply_snapshot(snapshot) {
                    eprintln!("{}", Text::VisiblePlayerLimitExceeded);
                    self.fail_connection(Text::VisiblePlayerLimitExceeded.to_string());
                }
            }
            ServerMessage::TransitionPrepare {
                transition_id,
                source_context,
                target_context,
                map_fingerprint,
                destination,
                manifest,
                ..
            } => {
                let instance = target_context.instance();
                if !self.load_generated_catalog(instance, manifest) {
                    self.fail_connection("target generation manifest is not supported");
                    return;
                }
                let matches_catalog = self
                    .world_catalog
                    .get(&instance)
                    .is_some_and(|map| map.fingerprint() == map_fingerprint)
                    && self.game.world_manifests.get(&instance) == Some(&manifest);
                if !matches_catalog {
                    self.fail_connection("target world content does not match the loaded manifest");
                    return;
                }
                self.pending_sector_transitions.insert(
                    transition_id,
                    PendingSectorTransition {
                        source_context,
                        context: target_context,
                        destination,
                        manifest,
                        ready_sent: false,
                    },
                );
                if self.ui.console.open {
                    self.ui.console.push_line("Preparing world transfer");
                }
                self.advance_pending_transitions();
            }
            ServerMessage::TransitionCommitted {
                context,
                map_fingerprint,
                destination,
                destination_mm,
                health,
                manifest,
                ..
            } => {
                if !self.load_generated_catalog(context.instance(), manifest) {
                    self.fail_connection("target generation manifest is not supported");
                    return;
                }
                if !self.install_world(
                    context.instance(),
                    map_fingerprint,
                    Some(destination),
                    manifest,
                ) {
                    self.fail_connection("target world geometry does not match the server");
                    return;
                }
                self.game.session_context = Some(context);
                self.game.health = health;
                self.game.position_mm = manifest.planet().map(|_| destination_mm);
                if self.ui.console.open {
                    self.ui.console.push_line("World transfer committed");
                }
            }
            ServerMessage::TransitionCancelled {
                transition_id,
                context,
            } => {
                if let Some(pending) = self
                    .pending_sector_transitions
                    .get(&transition_id)
                    .copied()
                    .filter(|pending| pending.source_context == context)
                {
                    self.pending_sector_transitions.remove(&transition_id);
                    self.release_prepared_target(pending.context.instance());
                }
            }
            _ => {}
        }
    }

    fn load_generated_catalog(
        &mut self,
        instance: minimium_core::world::WorldInstanceId,
        manifest: WorldManifest,
    ) -> bool {
        if !matches!(manifest.generation().revision().get(), 2..=4)
            || !matches!(instance.get(), 1 | 2)
        {
            return true;
        }
        let seed = manifest.generation().seed().get()
            ^ if instance.get() == 2 {
                0x0030_0000_0000_0003
            } else {
                0
            };
        let generated = if let Some(planet) = manifest.planet() {
            minimium_procgen::generate_worlds_with_planet_revision(
                seed,
                planet,
                manifest.generation().revision().get(),
            )
        } else {
            minimium_procgen::generate_worlds(seed)
        };
        let Ok(worlds) = generated else {
            return false;
        };
        if worlds[instance.get() as usize - 1].manifest != manifest {
            return false;
        }
        if self.game.world_manifests.get(&instance) != Some(&manifest) {
            self.sector_cache.reset();
            for generated_instance in [1, 2] {
                self.gpu.release_world_sectors(
                    minimium_core::world::WorldInstanceId::new(generated_instance)
                        .expect("generated instance ID is nonzero"),
                );
            }
        }
        for world in worlds {
            self.game
                .world_manifests
                .insert(world.instance, world.manifest);
            self.world_catalog.insert(world.instance, world.map);
        }
        true
    }

    fn install_world(
        &mut self,
        instance: minimium_core::world::WorldInstanceId,
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
        let Some(scene) = self.sector_cache.ensure_scene(
            instance,
            manifest,
            target_position,
            &self.world_catalog,
            &self.game.world_manifests,
        ) else {
            return false;
        };
        if !self.gpu.cache_sector_scene(instance, &scene)
            || !self.gpu.activate_sector_scene(instance, &scene)
        {
            return false;
        }
        let previous_instance = self.game.current_instance;
        self.game.collision_index = scene.collision.clone();
        self.game.character = CharacterState::at_spawn_with_gravity(
            target_position,
            super::camera::gravity_for_manifest(manifest),
        );
        self.game.position_mm = manifest.planet().map(|_| {
            if target_position == map.spawn() {
                map.spawn_position_mm().unwrap_or_else(|| {
                    minimium_core::world::Position3Mm(
                        target_position.map(|value| (f64::from(value) * 1000.0).round() as i64),
                    )
                })
            } else {
                minimium_core::world::Position3Mm(
                    target_position.map(|value| (f64::from(value) * 1000.0).round() as i64),
                )
            }
        });
        self.game.sectors_ready = true;
        self.active_sector_keys.clone_from(&scene.keys);
        self.last_render_position = target_position;
        self.game.physics_accumulator = 0.0;
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

    pub(in crate::renderer) fn refresh_active_sector_scene(&mut self) -> bool {
        let Some(manifest) = self
            .game
            .world_manifests
            .get(&self.game.current_instance)
            .copied()
        else {
            self.game.sectors_ready = false;
            return false;
        };
        let keys =
            minimium_procgen::sector_neighborhood_for(manifest, self.game.character.position);
        let render_distance_squared = self
            .game
            .character
            .position
            .iter()
            .zip(self.last_render_position)
            .map(|(current, previous)| (current - previous) * (current - previous))
            .sum::<f32>();
        if self.active_sector_keys == keys
            && render_distance_squared < 400.0
            && self
                .sector_cache
                .interest_matches(self.game.current_instance, &keys, manifest)
        {
            self.game.sectors_ready = true;
            return true;
        }
        let Some(scene) = self.sector_cache.ensure_scene(
            self.game.current_instance,
            manifest,
            self.game.character.position,
            &self.world_catalog,
            &self.game.world_manifests,
        ) else {
            self.game.sectors_ready = false;
            return false;
        };
        if !self
            .gpu
            .cache_sector_scene(self.game.current_instance, &scene)
            || !self
                .gpu
                .activate_sector_scene(self.game.current_instance, &scene)
        {
            self.game.sectors_ready = false;
            return false;
        }
        self.game.collision_index = scene.collision;
        self.active_sector_keys.clone_from(&scene.keys);
        self.last_render_position = self.game.character.position;
        self.game.sectors_ready = true;
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
                &self.world_catalog,
                &self.game.world_manifests,
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

    fn release_prepared_target(&mut self, instance: minimium_core::world::WorldInstanceId) {
        if instance != self.game.current_instance {
            self.sector_cache.release_interest(instance);
            self.gpu.release_world_sectors(instance);
        }
    }
}
