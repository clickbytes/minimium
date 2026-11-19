use super::*;
use crate::connection::queue_message;
use crate::world_inputs::{WorldEvent, WorldInput};
use minimium_protocol::{PlayerActionDecision, PlayerActionKind};

const INPUT_IDLE_TIMEOUT: Duration = Duration::from_millis(250);

pub(super) type ScheduledMotion = std::collections::BTreeMap<
    u64,
    (
        minimium_protocol::WorldContext,
        u64,
        minimium_protocol::MovementSample,
    ),
>;

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
                    .record(self.now.saturating_duration_since(decoded_at));
            }
            if let WorldInput::Move(player, context, sequence, sample) = &input {
                if teleported.contains(player) {
                    self.resolve_movement_input(*player, sample.input_seq);
                    self.send_player_action_receipt(
                        *player,
                        *sequence,
                        *context,
                        PlayerActionKind::Move,
                        PlayerActionDecision::Rejected(8),
                    );
                    continue;
                }
            }
            if let Some(event) = self.apply_input(input, &mut teleported) {
                self.emit_world_event(event);
            }
        }
        self.game.pending_world_inputs = pending;
        self.apply_scheduled_movement_inputs();
        let now = self.now;
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
        let tick = self.step_boundary().tick();
        let players: Vec<_> = self.game.pending_movement_inputs.keys().copied().collect();
        for player in players {
            let (current, due_receipts, empty) = {
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
                    .max_by_key(|(_, _, sample)| sample.input_seq);
                let receipts = due
                    .iter()
                    .map(|(_, (context, sequence, sample))| (*context, *sequence, sample.input_seq))
                    .collect::<Vec<_>>();
                for (target, _) in due {
                    samples.remove(&target);
                }
                (current, receipts, samples.is_empty())
            };
            if empty {
                self.game.pending_movement_inputs.remove(&player);
            }
            let committed = current.filter(|(context, _, _)| {
                self.game
                    .worlds
                    .world_input_context_is_current(player, *context)
            });
            if let Some((context, sequence, sample)) = committed {
                self.game
                    .applied_input
                    .insert(player, (sample.input_seq, tick));
                self.game.active_inputs.insert(player, sample.intent);
                self.send_player_action_receipt(
                    player,
                    sequence,
                    context,
                    PlayerActionKind::Move,
                    PlayerActionDecision::Committed,
                );
            }
            for (context, sequence, input_seq) in due_receipts {
                self.resolve_movement_input(player, input_seq);
                if committed.is_none_or(|(_, committed_sequence, _)| committed_sequence != sequence)
                {
                    self.send_player_action_receipt(
                        player,
                        sequence,
                        context,
                        PlayerActionKind::Move,
                        PlayerActionDecision::Rejected(
                            if self
                                .game
                                .worlds
                                .world_input_context_is_current(player, context)
                            {
                                8
                            } else {
                                6
                            },
                        ),
                    );
                }
            }
        }
    }

    pub(super) fn apply_input(
        &mut self,
        input: WorldInput,
        teleported: &mut std::collections::HashSet<minimium_core::ids::PlayerId>,
    ) -> Option<WorldEvent> {
        if !self.validate_player_input(&input) {
            let receipt = match &input {
                WorldInput::Move(id, context, sequence, _) => {
                    Some((*id, *context, *sequence, PlayerActionKind::Move))
                }
                WorldInput::Fire(id, context, sequence, _) => {
                    Some((*id, *context, *sequence, PlayerActionKind::Fire))
                }
                WorldInput::Chat(id, context, sequence, _) => {
                    Some((*id, *context, *sequence, PlayerActionKind::Chat))
                }
                WorldInput::Rename(id, context, sequence, _) => {
                    Some((*id, *context, *sequence, PlayerActionKind::Name))
                }
                WorldInput::GrantTools(id, context, sequence) => {
                    Some((*id, *context, *sequence, PlayerActionKind::GrantTools))
                }
                _ => None,
            };
            if let Some((id, context, sequence, kind)) = receipt {
                let reason = if self.game.worlds.world_input_context_is_current(id, context) {
                    1
                } else {
                    6
                };
                self.send_player_action_receipt(
                    id,
                    sequence,
                    context,
                    kind,
                    PlayerActionDecision::Rejected(reason),
                );
            }
            if let WorldInput::Move(id, _, _, sample) = input {
                self.resolve_movement_input(id, sample.input_seq);
            }
            return None;
        }
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
                self.game.input_last_received.insert(id, self.now);
            }
            WorldInput::Leave(id) => {
                self.abort_jumps(id, minimium_protocol::MotionRejection::StaleContext);
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
                self.game.remove_actor_actions(id);
                self.game.active_inputs.remove(&id);
                self.game.input_last_received.remove(&id);
                self.game.pending_movement_inputs.remove(&id);
                self.game.last_input_seq.remove(&id);
                self.game.pending_input_seqs.remove(&id);
                self.game.resolved_input_seq.remove(&id);
                self.game.applied_input.remove(&id);
                if let Some(profile) = self.operations.player_store.profile_for_player(id) {
                    self.game.pending_departures.remove(&profile);
                }
            }
            WorldInput::Move(id, context, sequence, mut sample) => {
                if let Some(ticket) = self.game.jumps.get_mut(&id).and_then(|tickets| {
                    tickets.iter_mut().find(|t| t.input_seq == sample.input_seq)
                }) {
                    ticket.scheduled = true;
                }
                sample.target_tick = self.step_boundary().tick();
                if self
                    .game
                    .pending_movement_inputs
                    .get(&id)
                    .and_then(|samples| samples.get(&sample.target_tick))
                    .is_some_and(|(old_context, _, old)| *old_context == context && old.intent.jump)
                {
                    sample.intent.jump = true;
                }
                let previous = self
                    .game
                    .pending_movement_inputs
                    .entry(id)
                    .or_default()
                    .insert(sample.target_tick, (context, sequence, sample));
                if let Some((previous_context, previous_sequence, previous)) =
                    previous.filter(|(_, _, previous)| previous.input_seq != sample.input_seq)
                {
                    self.resolve_movement_input(id, previous.input_seq);
                    self.send_player_action_receipt(
                        id,
                        previous_sequence,
                        previous_context,
                        PlayerActionKind::Move,
                        PlayerActionDecision::Rejected(8),
                    );
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
                    self.abort_jumps(id, minimium_protocol::MotionRejection::Superseded);
                    if let Some(samples) = self.game.pending_movement_inputs.remove(&id) {
                        for (sample_context, sample_sequence, sample) in samples.values() {
                            self.resolve_movement_input(id, sample.input_seq);
                            self.send_player_action_receipt(
                                id,
                                *sample_sequence,
                                *sample_context,
                                PlayerActionKind::Move,
                                PlayerActionDecision::Rejected(8),
                            );
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
            WorldInput::Excavation(id, context, input) => {
                self.apply_excavation(id, context, input);
            }
            WorldInput::GrantTools(id, context, sequence) => {
                if self.change_inventory(
                    id,
                    context,
                    minimium_protocol::ExcavationInput::GrantTools,
                ) {
                    if let Some(profile) = self.player_profile(id) {
                        let desired = self.equipment.desired(id, context, profile);
                        if let Some((old_context, old_sequence, _)) = self
                            .game
                            .pending_grant_receipts
                            .insert(id, (context, sequence, desired))
                        {
                            self.send_player_action_receipt(
                                id,
                                old_sequence,
                                old_context,
                                PlayerActionKind::GrantTools,
                                PlayerActionDecision::Rejected(8),
                            );
                        }
                        self.resolve_grant_receipts();
                    }
                } else {
                    self.send_player_action_receipt(
                        id,
                        sequence,
                        context,
                        PlayerActionKind::GrantTools,
                        PlayerActionDecision::Rejected(5),
                    );
                }
            }
            WorldInput::Fire(id, context, sequence, direction) => {
                let event = self.apply_fire_input(id, direction);
                self.send_player_action_receipt(
                    id,
                    sequence,
                    context,
                    PlayerActionKind::Fire,
                    if event.is_some() {
                        PlayerActionDecision::Committed
                    } else {
                        PlayerActionDecision::Rejected(5)
                    },
                );
                return event;
            }
            WorldInput::Chat(id, context, sequence, text) => {
                if !self.operations.player_store.is_active(id) {
                    self.send_player_action_receipt(
                        id,
                        sequence,
                        context,
                        PlayerActionKind::Chat,
                        PlayerActionDecision::Rejected(5),
                    );
                    return None;
                }
                let Some(instance) = self.game.worlds.instance_for_player(id) else {
                    self.send_player_action_receipt(
                        id,
                        sequence,
                        context,
                        PlayerActionKind::Chat,
                        PlayerActionDecision::Rejected(5),
                    );
                    return None;
                };
                let Some(name) = self
                    .operations
                    .player_store
                    .name_for_player(id)
                    .map(str::to_owned)
                else {
                    self.send_player_action_receipt(
                        id,
                        sequence,
                        context,
                        PlayerActionKind::Chat,
                        PlayerActionDecision::Rejected(5),
                    );
                    return None;
                };
                self.send_player_action_receipt(
                    id,
                    sequence,
                    context,
                    PlayerActionKind::Chat,
                    PlayerActionDecision::Committed,
                );
                return Some(WorldEvent::Chat {
                    instance,
                    sender: id,
                    name,
                    text,
                });
            }
            WorldInput::Rename(id, context, sequence, name) => {
                let Some(world) = self.game.worlds.for_player_mut(id) else {
                    self.send_player_action_receipt(
                        id,
                        sequence,
                        context,
                        PlayerActionKind::Name,
                        PlayerActionDecision::Rejected(5),
                    );
                    return None;
                };
                if !world.world.rename_player(id, name.clone()) {
                    self.send_player_action_receipt(
                        id,
                        sequence,
                        context,
                        PlayerActionKind::Name,
                        PlayerActionDecision::Rejected(5),
                    );
                    return None;
                }
                self.operations.player_store.rename(id, name);
                self.send_player_action_receipt(
                    id,
                    sequence,
                    context,
                    PlayerActionKind::Name,
                    PlayerActionDecision::Committed,
                );
            }
            WorldInput::InteractEntity(id, source_context, entity) => {
                self.publish_entity_interaction(id, source_context, entity);
            }
        }
        None
    }
}
