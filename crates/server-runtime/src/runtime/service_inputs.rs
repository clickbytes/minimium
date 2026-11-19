use super::*;
use crate::world_inputs::WorldInput;

impl ServerRuntime {
    pub(super) fn service_inputs(&mut self) {
        let paused = self.environment_clock.rate().get() == 0;
        let mut pending = std::mem::take(&mut self.game.pending_world_inputs);
        let mut teleported = HashSet::new();
        for (input, at) in pending.drain(..) {
            match input {
                WorldInput::Join { .. }
                | WorldInput::Leave(_)
                | WorldInput::Chat(..)
                | WorldInput::Rename(..) => {
                    if let Some(event) = self.apply_input(input, &mut teleported) {
                        self.emit_world_event(event);
                    }
                }
                _ if !paused => self.game.pending_world_inputs.push((input, at)),
                WorldInput::Move(player, context, sequence, sample) => {
                    self.resolve_movement_input(player, sample.input_seq);
                    self.send_player_action_receipt(
                        player,
                        sequence,
                        context,
                        minimium_protocol::PlayerActionKind::Move,
                        minimium_protocol::PlayerActionDecision::Rejected(7),
                    );
                }
                WorldInput::GrantTools(player, context, sequence) => {
                    self.send_player_action_receipt(
                        player,
                        sequence,
                        context,
                        minimium_protocol::PlayerActionKind::GrantTools,
                        minimium_protocol::PlayerActionDecision::Rejected(7),
                    );
                }
                WorldInput::Fire(player, context, sequence, _) => {
                    self.send_player_action_receipt(
                        player,
                        sequence,
                        context,
                        minimium_protocol::PlayerActionKind::Fire,
                        minimium_protocol::PlayerActionDecision::Rejected(7),
                    );
                }
                WorldInput::InteractEntity(player, context, sequence, _) => {
                    self.send_player_action_receipt(
                        player,
                        sequence,
                        context,
                        minimium_protocol::PlayerActionKind::Portal,
                        minimium_protocol::PlayerActionDecision::Rejected(7),
                    );
                }
                WorldInput::MapTeleport(player, context, sequence, _) => {
                    self.send_player_action_receipt(
                        player,
                        sequence,
                        context,
                        minimium_protocol::PlayerActionKind::Goto,
                        minimium_protocol::PlayerActionDecision::Rejected(7),
                    );
                }
                WorldInput::SetAltitude(player, context, sequence, _) => {
                    self.send_player_action_receipt(
                        player,
                        sequence,
                        context,
                        minimium_protocol::PlayerActionKind::Altitude,
                        minimium_protocol::PlayerActionDecision::Rejected(7),
                    );
                }
                WorldInput::Excavation(
                    player,
                    context,
                    minimium_protocol::ExcavationInput::Use { action_id, .. },
                ) => {
                    self.excavation_result(
                        player,
                        context,
                        minimium_world::items::ActionResult::Rejected {
                            action_id,
                            reason: minimium_world::items::ActionRejection::Paused,
                        },
                    );
                }
                WorldInput::Excavation(player, ..) => {
                    if let Some(&index) = self.connections.client_indices_by_player.get(&player) {
                        crate::connection::queue_message(
                            &mut self.connections.clients[index],
                            &mut self.metrics,
                            minimium_protocol::ServerMessage::Error(
                                minimium_protocol::ServerErrorCode::Paused,
                            ),
                        );
                    }
                }
            }
        }
        if paused {
            let players: Vec<_> = self.game.jumps.keys().copied().collect();
            for id in players {
                self.abort_jumps(id, minimium_protocol::MotionRejection::Paused);
            }
            self.game.pending_movement_inputs.clear();
            self.game.pending_input_seqs.clear();
            self.game
                .resolved_input_seq
                .clone_from(&self.game.last_input_seq);
            self.game
                .active_inputs
                .values_mut()
                .for_each(|intent| *intent = MovementIntent::default());
        }
    }

    pub(super) fn publish_entity_interaction(
        &mut self,
        id: minimium_core::ids::PlayerId,
        source_context: minimium_protocol::WorldContext,
        entity: minimium_world::entity::EntityId,
    ) -> bool {
        if self.game.gameplay_events.len() < crate::MAX_PENDING_WORLD_INPUTS {
            if let Some(position) = self
                .game
                .worlds
                .player_position_in_context(id, source_context)
            {
                self.game.gameplay_events.push(
                    crate::modules::GameplayEvent::EntityInteractionRequested {
                        player: id,
                        world: source_context.instance(),
                        entity,
                        position,
                    },
                );
                return true;
            }
        }
        false
    }
}
