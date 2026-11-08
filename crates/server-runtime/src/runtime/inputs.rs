use super::*;
use crate::connection::queue_message;
use crate::world_inputs::{WorldEvent, WorldInput};

const INPUT_IDLE_TIMEOUT: Duration = Duration::from_millis(250);

impl ServerRuntime {
    pub(super) fn resolve_movement_input(
        &mut self,
        player: minimium_core::ids::PlayerId,
        input_seq: u64,
    ) {
        if let Some(pending) = self.game.pending_input_seqs.get_mut(&player) {
            pending.remove(&input_seq);
            if pending.is_empty() {
                self.game.pending_input_seqs.remove(&player);
            }
        }
        let first_pending = self
            .game
            .pending_input_seqs
            .get(&player)
            .and_then(|set| set.first().copied());
        let resolved = first_pending.map_or_else(
            || self.game.last_input_seq.get(&player).copied().unwrap_or(0),
            |first| first.saturating_sub(1),
        );
        self.game
            .resolved_input_seq
            .entry(player)
            .and_modify(|seq| *seq = (*seq).max(resolved))
            .or_insert(resolved);
    }

    pub(super) fn apply_inputs(&mut self) {
        // Reuse the allocation and deliver each event before the next input.
        let mut pending = std::mem::take(&mut self.game.pending_world_inputs);
        let mut teleported = std::collections::HashSet::new();
        for (input, decoded_at) in pending.drain(..) {
            if let Some(decoded_at) = decoded_at {
                self.metrics
                    .input_wait
                    .record(Instant::now().saturating_duration_since(decoded_at));
            }
            if let WorldInput::Move(player, sample) = &input {
                if teleported.contains(player) {
                    self.resolve_movement_input(*player, sample.input_seq);
                    continue;
                }
            }
            if let Some(event) = self.apply_input(input, &mut teleported) {
                self.emit_world_event(event);
            }
        }
        self.game.pending_world_inputs = pending;
        self.apply_scheduled_movement_inputs();
        let now = Instant::now();
        for (id, last_received) in &self.game.input_last_received {
            if now.duration_since(*last_received) > INPUT_IDLE_TIMEOUT {
                if let Some(input) = self.game.active_inputs.get_mut(id) {
                    input.direction = [0.0; 2];
                    input.vertical = 0.0;
                }
            }
        }
    }

    fn apply_scheduled_movement_inputs(&mut self) {
        let tick = self.scheduler.simulation_tick;
        let players: Vec<_> = self.game.pending_movement_inputs.keys().copied().collect();
        for player in players {
            let (current, due_sequences, empty) = {
                let Some(samples) = self.game.pending_movement_inputs.get_mut(&player) else {
                    continue;
                };
                let due: Vec<_> = samples
                    .range(..=tick)
                    .map(|(target, sample)| (*target, *sample))
                    .collect();
                let current = due
                    .iter()
                    .filter(|(target, _)| *target == tick)
                    .map(|(_, sample)| *sample)
                    .max_by_key(|sample| sample.input_seq);
                let sequences = due
                    .iter()
                    .map(|(_, sample)| sample.input_seq)
                    .collect::<Vec<_>>();
                for (target, _) in due {
                    samples.remove(&target);
                }
                (current, sequences, samples.is_empty())
            };
            if empty {
                self.game.pending_movement_inputs.remove(&player);
            }
            if let Some(sample) = current {
                self.game.active_inputs.insert(player, sample.intent);
                self.game.input_last_received.insert(player, Instant::now());
            }
            for seq in due_sequences {
                self.resolve_movement_input(player, seq);
            }
        }
    }

    fn apply_input(
        &mut self,
        input: WorldInput,
        teleported: &mut std::collections::HashSet<minimium_core::ids::PlayerId>,
    ) -> Option<WorldEvent> {
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
                    crate::diagnostics::emit(
                        &self.world_logger,
                        minimium_diagnostics::Level::Warn,
                        "world.player_join_rejected",
                        &[
                            ("player_id", id.to_string()),
                            ("world_instance", format!("{instance:?}")),
                            ("reason", format!("{error:?}")),
                        ],
                    );
                    return None;
                }
                self.game
                    .active_inputs
                    .insert(id, MovementIntent::default());
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
                                crate::diagnostics::emit(
                                    &self.server_logger,
                                    minimium_diagnostics::Level::Error,
                                    "server.player_leave_save_failed",
                                    &[("player_id", id.to_string()), ("reason", error.to_string())],
                                );
                            }
                        }
                    }
                }
                self.game.worlds.leave(id);
                self.game.active_inputs.remove(&id);
                self.game.input_last_received.remove(&id);
                self.game.pending_movement_inputs.remove(&id);
                self.game.last_input_seq.remove(&id);
                self.game.pending_input_seqs.remove(&id);
                self.game.resolved_input_seq.remove(&id);
                if let Some(profile) = self.operations.player_store.profile_for_player(id) {
                    self.game.pending_departures.remove(&profile);
                }
            }
            WorldInput::Move(id, sample) => {
                let previous = self
                    .game
                    .pending_movement_inputs
                    .entry(id)
                    .or_default()
                    .insert(sample.target_tick, sample);
                if let Some(previous) =
                    previous.filter(|previous| previous.input_seq != sample.input_seq)
                {
                    self.resolve_movement_input(id, previous.input_seq);
                }
                self.game
                    .last_input_seq
                    .entry(id)
                    .and_modify(|seq| *seq = (*seq).max(sample.input_seq))
                    .or_insert(sample.input_seq);
            }
            WorldInput::SetAltitude(id, context, km) => self.apply_altitude(id, context, km),
            WorldInput::MapTeleport(id, context, direction) => {
                let position_mm = self.map_teleport(id, context, direction);
                if position_mm.is_some() {
                    teleported.insert(id);
                    if let Some(samples) = self.game.pending_movement_inputs.remove(&id) {
                        for sample in samples.values() {
                            self.resolve_movement_input(id, sample.input_seq);
                        }
                    }
                }
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
                let outcome = world.fire_hitscan(id, direction, self.combat)?;
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
            WorldInput::InteractEntity(id, source_context, entity) => {
                self.publish_portal_use(id, source_context, entity);
            }
        }
        None
    }

    fn publish_portal_use(
        &mut self,
        id: minimium_core::ids::PlayerId,
        source_context: minimium_protocol::WorldContext,
        entity: minimium_world::entity::EntityId,
    ) {
        if self.game.gameplay_events.len() < crate::MAX_PENDING_WORLD_INPUTS {
            if let Some(position) = self
                .game
                .worlds
                .player_position_in_context(id, source_context)
            {
                self.game
                    .gameplay_events
                    .push(crate::modules::GameplayEvent::PortalUseRequested {
                        player: id,
                        world: source_context.instance(),
                        entity,
                        position,
                    });
            }
        }
    }

    pub(super) fn begin_entity_transfer(
        &mut self,
        id: minimium_core::ids::PlayerId,
        source_context: minimium_protocol::WorldContext,
        target_world: minimium_world::world::WorldInstanceId,
        destination: [f32; 3],
    ) {
        let Some(offer) = self.game.worlds.prepare_entity_transfer(
            id,
            source_context,
            target_world,
            destination,
            self.operations.player_store.return_location_for(id),
        ) else {
            return;
        };
        let Some(index) = self.connections.client_indices_by_player.get(&id).copied() else {
            self.game.worlds.cancel_transfer(id, offer.transition_id);
            return;
        };
        let (clock_sample, environment) = match self
            .environment_state_for_instance(offer.target_context.instance(), Instant::now())
        {
            Ok(state) => state,
            Err(error) => {
                crate::diagnostics::emit(
                    &self.server_logger,
                    minimium_diagnostics::Level::Error,
                    "server.transfer_environment_failed",
                    &[
                        ("player_id", id.to_string()),
                        ("error_code", error.code().to_owned()),
                        ("reason", error.to_string()),
                    ],
                );
                self.game.worlds.cancel_transfer(id, offer.transition_id);
                return;
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
                self.metrics.dropped_messages = self.metrics.dropped_messages.saturating_add(1);
            }
        }
    }

    fn map_teleport(
        &mut self,
        id: minimium_core::ids::PlayerId,
        context: minimium_protocol::WorldContext,
        direction: [f32; 3],
    ) -> Option<minimium_world::world::Position3Mm> {
        if !self.game.worlds.can_map_teleport(id, context) {
            return None;
        }
        let instance = self.game.worlds.instance_for_player(id)?;
        let manifest = self.game.worlds.manifest_for_player(id)?;
        if manifest.planet()?.mode != 1 {
            return None;
        }
        let point = minimium_procgen::planet_point_at_direction_with_geography(
            manifest,
            direction,
            self.prepared_geography.clone(),
        )?;
        if point.ocean {
            return None;
        }
        let ground = minimium_procgen::planet_collision_position_at_direction_with_geography(
            manifest,
            direction,
            self.prepared_geography.clone(),
        )?;
        let length = ground
            .iter()
            .map(|value| (*value as f64).powi(2))
            .sum::<f64>()
            .sqrt();
        let position_mm = minimium_world::world::Position3Mm(std::array::from_fn(|axis| {
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
                crate::diagnostics::emit(
                    &self.server_logger,
                    minimium_diagnostics::Level::Error,
                    "server.teleport_save_failed",
                    &[("player_id", id.to_string()), ("reason", error.to_string())],
                );
            }
        }
        self.game
            .active_inputs
            .insert(id, minimium_simulation::MovementIntent::default());
        self.game.input_last_received.insert(id, Instant::now());
        self.scheduler.next_snapshot = Instant::now();
        Some(position_mm)
    }
}
