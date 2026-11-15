use super::*;
use crate::{
    connection::{queue_message, queue_message_with_reserve},
    world_inputs::WorldEvent,
};
use minimium_protocol::ServerMessage;

impl ServerRuntime {
    pub(super) fn emit_world_event(&mut self, event: WorldEvent) {
        match event {
            WorldEvent::Shot {
                instance,
                shooter,
                outcome,
                target_health,
            } => {
                if let (Some(target), Some(health)) = (outcome.target_id, target_health) {
                    if let Some(index) = self
                        .connections
                        .client_indices_by_player
                        .get(&target)
                        .copied()
                    {
                        if let Some(client) =
                            self.connections.clients.get_mut(index).filter(|client| {
                                client.player_id() == Some(target)
                                    && self.game.worlds.instance_for_player(target)
                                        == Some(instance)
                            })
                        {
                            client.pending_health_update = Some(health);
                        }
                    }
                }
                if let Some(index) = self
                    .connections
                    .client_indices_by_player
                    .get(&shooter)
                    .copied()
                {
                    let message = ServerMessage::ShotResult {
                        target_id: outcome.target_id,
                        remaining_health: outcome.remaining_health,
                        killed: outcome.killed,
                    };
                    if let Some(client) = self.connections.clients.get_mut(index).filter(|client| {
                        client.player_id() == Some(shooter)
                            && self.game.worlds.instance_for_player(shooter) == Some(instance)
                    }) {
                        if !queue_message(client, &mut self.metrics, message) {
                            self.metrics.dropped_messages =
                                self.metrics.dropped_messages.saturating_add(1);
                        }
                    }
                }
            }
            WorldEvent::Chat {
                instance,
                sender,
                name,
                text,
            } => {
                let message = ServerMessage::Chat {
                    sender_id: sender,
                    sender_name: name,
                    text,
                };
                for client in &mut self.connections.clients {
                    if client.player_id().is_none_or(|player| {
                        self.game.worlds.instance_for_player(player) != Some(instance)
                    }) {
                        continue;
                    }
                    if !queue_message_with_reserve(client, &mut self.metrics, message.clone(), 0) {
                        self.metrics.dropped_messages =
                            self.metrics.dropped_messages.saturating_add(1);
                    }
                }
            }
        }
    }
}
