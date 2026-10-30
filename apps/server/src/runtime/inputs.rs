use super::*;
use crate::connection::queue_message;
use crate::world_inputs::{WorldEvent, WorldInput};

const INPUT_IDLE_TIMEOUT: Duration = Duration::from_millis(250);

impl ServerRuntime {
    pub(super) fn apply_inputs(&mut self) {
        // Reuse the allocation and deliver each event before the next input.
        let mut pending = std::mem::take(&mut self.game.pending_world_inputs);
        for (input, decoded_at) in pending.drain(..) {
            if let Some(decoded_at) = decoded_at {
                self.metrics
                    .input_wait
                    .record(Instant::now().saturating_duration_since(decoded_at));
            }
            if let Some(event) = self.apply_input(input) {
                self.emit_world_event(event);
            }
        }
        self.game.pending_world_inputs = pending;
        let now = Instant::now();
        for (id, last_received) in &self.game.input_last_received {
            if now.duration_since(*last_received) > INPUT_IDLE_TIMEOUT {
                if let Some(input) = self.game.active_inputs.get_mut(id) {
                    input.direction = [0.0; 2];
                }
            }
        }
    }

    fn apply_input(&mut self, input: WorldInput) -> Option<WorldEvent> {
        match input {
            WorldInput::Join {
                id,
                instance,
                name,
                position,
                position_mm,
                health,
            } => {
                if let Err(error) = self.game.worlds.join_at_precise(
                    instance,
                    id,
                    name,
                    position,
                    position_mm,
                    health,
                ) {
                    eprintln!("could not add player {id} to instance {instance}: {error:?}");
                    return None;
                }
                self.game
                    .active_inputs
                    .insert(id, CharacterInput::default());
                self.game.input_last_received.insert(id, Instant::now());
            }
            WorldInput::Leave(id) => {
                if let Some(instance_id) = self.game.worlds.instance_for_player(id) {
                    if let Some(instance) = self.game.worlds.get(instance_id) {
                        let gravity = instance.world.gravity();
                        if let Some(player) = instance.world.player(id) {
                            if let Err(error) = self.operations.player_store.update_from_player(
                                player,
                                instance_id,
                                gravity,
                            ) {
                                eprintln!("could not capture player {} before leave: {error}", id);
                            }
                        }
                    }
                }
                self.game.worlds.leave(id);
                self.game.active_inputs.remove(&id);
                self.game.input_last_received.remove(&id);
                if let Some(profile) = self.operations.player_store.profile_for_player(id) {
                    self.game.pending_departures.remove(&profile);
                }
            }
            WorldInput::Move(id, input) => {
                let current = self.game.active_inputs.entry(id).or_default();
                current.direction = input.direction;
                current.jump |= input.jump;
                self.game.input_last_received.insert(id, Instant::now());
            }
            WorldInput::MapTeleport(id, context, direction) => {
                let position_mm = self.map_teleport(id, context, direction);
                if let Some(index) = self.connections.client_indices_by_player.get(&id).copied() {
                    if let Some(client) = self.connections.clients.get_mut(index) {
                        if !queue_message(
                            client,
                            &mut self.metrics,
                            minimium_protocol::ServerMessage::MapTeleportResult { position_mm },
                        ) {
                            self.metrics.dropped_messages =
                                self.metrics.dropped_messages.saturating_add(1);
                            client.disconnect_reason =
                                Some(crate::connection::CloseReason::Backpressure);
                            self.connections.disconnected.push(index);
                        }
                    }
                }
            }
            WorldInput::Fire(id, direction) => {
                self.metrics.shots_fired = self.metrics.shots_fired.saturating_add(1);
                let instance = self.game.worlds.instance_for_player(id)?;
                let world = &mut self.game.worlds.get_mut(instance)?.world;
                let outcome = world.fire_hitscan(id, direction)?;
                let target_health = outcome
                    .target_id
                    .and_then(|target| world.player_health(target));
                if outcome.target_id.is_some() {
                    self.metrics.shots_hit = self.metrics.shots_hit.saturating_add(1);
                    if outcome.killed {
                        self.metrics.player_kills = self.metrics.player_kills.saturating_add(1);
                    }
                }
                return Some(WorldEvent::Shot {
                    instance,
                    shooter: id,
                    outcome,
                    target_health,
                });
            }
            WorldInput::Chat(id, text) => {
                if !self.operations.player_store.is_active(id) {
                    return None;
                }
                let instance = self.game.worlds.instance_for_player(id)?;
                let name = self.operations.player_store.name_for_player(id)?.to_owned();
                return Some(WorldEvent::Chat {
                    instance,
                    sender: id,
                    name,
                    text,
                });
            }
            WorldInput::Rename(id, name) => {
                self.game
                    .worlds
                    .for_player_mut(id)?
                    .world
                    .rename_player(id, name.clone());
                self.operations.player_store.rename(id, name);
            }
            WorldInput::UsePortal(id, source_context) => {
                let offer = self.game.worlds.prepare_portal_transfer(
                    id,
                    source_context,
                    self.operations.player_store.return_location_for(id),
                )?;
                let Some(index) = self.connections.client_indices_by_player.get(&id).copied()
                else {
                    self.game.worlds.cancel_transfer(id, offer.transition_id);
                    return None;
                };
                let (clock_sample, environment) = match self
                    .environment_state_for_instance(offer.target_context.instance(), Instant::now())
                {
                    Ok(state) => state,
                    Err(error) => {
                        eprintln!("could not sample target environment for transfer: {error}");
                        self.game.worlds.cancel_transfer(id, offer.transition_id);
                        return None;
                    }
                };
                if let Some(client) = self.connections.clients.get_mut(index) {
                    if !queue_message(
                        client,
                        &mut self.metrics,
                        minimium_protocol::ServerMessage::TransitionPrepare {
                            transition_id: offer.transition_id,
                            source_context: offer.source_context,
                            target_context: offer.target_context,
                            map_fingerprint: offer.map_fingerprint,
                            destination: offer.destination,
                            destination_mm: offer.destination_mm,
                            manifest: offer.manifest,
                            clock_sample,
                            environment,
                        },
                    ) {
                        self.game.worlds.cancel_transfer(id, offer.transition_id);
                        self.metrics.dropped_messages =
                            self.metrics.dropped_messages.saturating_add(1);
                    }
                }
            }
        }
        None
    }

    fn map_teleport(
        &mut self,
        id: minimium_core::ids::PlayerId,
        context: minimium_protocol::WorldContext,
        direction: [f32; 3],
    ) -> Option<minimium_core::world::Position3Mm> {
        if !self.game.worlds.can_map_teleport(id, context) {
            return None;
        }
        let instance = self.game.worlds.instance_for_player(id)?;
        let manifest = self.game.worlds.manifest_for_player(id)?;
        if manifest.planet()?.mode != 1 {
            return None;
        }
        let point = minimium_procgen::planet_point_at_direction(manifest, direction)?;
        if point.ocean {
            return None;
        }
        let ground = minimium_procgen::planet_collision_position_at_direction(manifest, direction)?;
        let length = ground
            .iter()
            .map(|value| (*value as f64).powi(2))
            .sum::<f64>()
            .sqrt();
        let position_mm = minimium_core::world::Position3Mm(std::array::from_fn(|axis| {
            ground[axis] + (ground[axis] as f64 / length * 2_000.0).round() as i64
        }));
        let position = position_mm.0.map(|value| value as f32 / 1000.0);
        let world = &mut self.game.worlds.get_mut(instance)?.world;
        if !world.set_position(id, position) || !world.restore_position_mm(id, position_mm) {
            return None;
        }
        if let Some(player) = world.player(id) {
            if let Err(error) =
                self.operations
                    .player_store
                    .update_from_player(player, instance, world.gravity())
            {
                eprintln!("could not record map teleport for player {id}: {error}");
            }
        }
        self.game
            .active_inputs
            .insert(id, minimium_core::world::CharacterInput::default());
        self.game.input_last_received.insert(id, Instant::now());
        self.scheduler.next_snapshot = Instant::now();
        Some(position_mm)
    }
}
