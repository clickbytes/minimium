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
                manifest,
                ..
            } => {
                if !self.install_world(
                    context.instance(),
                    map_fingerprint,
                    Some(position),
                    manifest,
                ) {
                    self.fail_connection("saved world instance is not available on this client");
                    return;
                }
                self.game.health = health;
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
                    self_velocity,
                    self_grounded,
                    players,
                    removed_players,
                };
                if !self.game.apply_snapshot(snapshot) {
                    eprintln!("{}", Text::VisiblePlayerLimitExceeded);
                    self.fail_connection(Text::VisiblePlayerLimitExceeded.to_string());
                }
            }
            ServerMessage::TransitionPrepare {
                transition_id,
                target_context,
                map_fingerprint,
                destination,
                manifest,
                ..
            } => {
                let instance = target_context.instance();
                let matches_catalog = self
                    .world_catalog
                    .get(&instance)
                    .is_some_and(|map| map.fingerprint() == map_fingerprint)
                    && self.world_manifests.get(&instance) == Some(&manifest);
                if !matches_catalog {
                    self.fail_connection("target world content does not match the loaded manifest");
                    return;
                }
                self.pending_sector_transitions.insert(
                    transition_id,
                    PendingSectorTransition {
                        context: target_context,
                        destination,
                        manifest,
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
                health,
                manifest,
                ..
            } => {
                if !self.install_world(
                    context.instance(),
                    map_fingerprint,
                    Some(destination),
                    manifest,
                ) {
                    self.fail_connection("target world geometry does not match the server");
                    return;
                }
                self.game.health = health;
                if self.ui.console.open {
                    self.ui.console.push_line("World transfer committed");
                }
            }
            ServerMessage::TransitionCancelled {
                transition_id,
                context,
            } if self
                .pending_sector_transitions
                .remove(&transition_id)
                .is_some()
                && context.instance() != self.current_instance =>
            {
                self.sector_cache.release_interest(context.instance());
                self.gpu.release_world_sectors(context.instance());
            }
            ServerMessage::TransitionCancelled { .. } => {}
            _ => {}
        }
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
        if self.world_manifests.get(&instance) != Some(&manifest) {
            return false;
        }
        let target_position = destination.unwrap_or_else(|| map.spawn());
        let Some(scene) = self.sector_cache.ensure_scene(
            instance,
            manifest,
            target_position,
            &self.world_catalog,
            &self.world_manifests,
        ) else {
            return false;
        };
        if !self.gpu.cache_sector_scene(instance, &scene)
            || !self.gpu.activate_sector_scene(instance, &scene)
        {
            return false;
        }
        let previous_instance = self.current_instance;
        self.game.collision_index = scene.collision.clone();
        self.game.character = CharacterState::at_spawn_with_gravity(
            target_position,
            super::camera::gravity_for_manifest(manifest),
        );
        self.game.sectors_ready = true;
        self.active_sector_keys.clone_from(&scene.keys);
        self.game.physics_accumulator = 0.0;
        self.reset_remote_gameplay();
        self.input.yaw = -std::f32::consts::FRAC_PI_2;
        self.input.pitch = 0.0;
        self.input.keys.clear();
        self.input.jump_pending = false;
        self.input.last_sent_direction = [0.0; 2];
        self.input.input_keepalive = 0.0;
        self.current_instance = instance;
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
        let Some(manifest) = self.world_manifests.get(&self.current_instance).copied() else {
            self.game.sectors_ready = false;
            return false;
        };
        let keys = minimium_procgen::sector_neighborhood(
            manifest.topology(),
            self.game.character.position,
        );
        if self.active_sector_keys == keys
            && self
                .sector_cache
                .interest_matches(self.current_instance, &keys, manifest)
        {
            self.game.sectors_ready = true;
            return true;
        }
        let Some(scene) = self.sector_cache.ensure_scene(
            self.current_instance,
            manifest,
            self.game.character.position,
            &self.world_catalog,
            &self.world_manifests,
        ) else {
            self.game.sectors_ready = false;
            return false;
        };
        if !self.gpu.cache_sector_scene(self.current_instance, &scene)
            || !self
                .gpu
                .activate_sector_scene(self.current_instance, &scene)
        {
            self.game.sectors_ready = false;
            return false;
        }
        self.game.collision_index = scene.collision;
        self.active_sector_keys.clone_from(&scene.keys);
        self.game.sectors_ready = true;
        true
    }

    pub(in crate::renderer) fn advance_pending_transitions(&mut self) {
        let pending: Vec<_> = self
            .pending_sector_transitions
            .iter()
            .map(|(id, pending)| (*id, *pending))
            .collect();
        for (transition_id, pending) in pending {
            let instance = pending.context.instance();
            let Some(scene) = self.sector_cache.ensure_scene(
                instance,
                pending.manifest,
                pending.destination,
                &self.world_catalog,
                &self.world_manifests,
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
                self.pending_sector_transitions.remove(&transition_id);
            }
        }
    }
}
