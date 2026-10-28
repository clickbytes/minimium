use super::*;
use crate::{
    connection::{queue_encoded_frame, queue_message},
    world_inputs::WorldEvent,
};
use minimium_protocol::{encode_server, ServerMessage};

impl ServerRuntime {
    pub(super) fn emit_world_event(&mut self, event: WorldEvent) {
        match event {
            WorldEvent::Shot {
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
                        if let Some(client) = self
                            .connections
                            .clients
                            .get_mut(index)
                            .filter(|client| client.player_id() == Some(target))
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
                    if let Some(client) = self
                        .connections
                        .clients
                        .get_mut(index)
                        .filter(|client| client.player_id() == Some(shooter))
                    {
                        if !queue_message(client, &mut self.metrics, message) {
                            self.metrics.dropped_messages =
                                self.metrics.dropped_messages.saturating_add(1);
                        }
                    }
                }
            }
            WorldEvent::Chat { sender, name, text } => {
                let message = ServerMessage::Chat {
                    sender_id: sender,
                    sender_name: name,
                    text,
                };
                let encoding_started = Instant::now();
                let encoded = encode_server(&message);
                self.metrics.encoding.record(encoding_started.elapsed());
                let Ok(frame) = encoded else {
                    self.metrics.dropped_messages = self.metrics.dropped_messages.saturating_add(1);
                    return;
                };
                for client in &mut self.connections.clients {
                    if !queue_encoded_frame(client, &frame, &mut self.metrics) {
                        self.metrics.dropped_messages =
                            self.metrics.dropped_messages.saturating_add(1);
                    }
                }
            }
        }
    }
}
