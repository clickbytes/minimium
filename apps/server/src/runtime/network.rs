use super::*;
use crate::{
    accept,
    connection::CLIENT_CLOSE_TIMEOUT,
    message_dispatch::{self, DispatchContext},
    receive, send,
    world_inputs::WorldInput,
};
use minimium_telemetry::allocations::{self, Phase};

impl ServerRuntime {
    pub(super) fn poll_network(&mut self) -> std::io::Result<()> {
        let network_phase = allocations::enter(Phase::Network);
        let network_started = Instant::now();
        accept::accept_clients(
            &self.game_listener,
            &mut self.connections.clients,
            self.max_client_connections,
            self.game.pending_world_inputs.len(),
        )?;

        receive::poll_clients(
            &mut self.connections.clients,
            &mut self.metrics,
            &mut self.connections.messages,
            &mut self.connections.disconnected,
        );
        message_dispatch::dispatch_messages(DispatchContext {
            clients: &mut self.connections.clients,
            disconnected: &mut self.connections.disconnected,
            player_store: &mut self.operations.player_store,
            max_players: self.max_players,
            pending_world_inputs: &mut self.game.pending_world_inputs,
            pending_departures: &self.game.pending_departures,
            next_player: &mut self.operations.next_player,
            map: &self.map,
            client_indices_by_player: &mut self.connections.client_indices_by_player,
            metrics: &mut self.metrics,
            last_global_chat_at: &mut self.game.last_global_chat_at,
            messages: &mut self.connections.messages,
        });
        let eof_close_at = Instant::now();
        for client in &mut self.connections.clients {
            if client.peer_eof && client.close_deadline.is_none() {
                client.close_deadline = Some(eof_close_at + CLIENT_CLOSE_TIMEOUT);
            }
        }
        let activity_check_at = Instant::now();
        for (index, client) in self.connections.clients.iter().enumerate() {
            if client.close_deadline.is_some() {
                continue;
            }
            let handshake_timed_out = client.player_id.is_none()
                && activity_check_at.duration_since(client.connected_at)
                    >= CLIENT_HANDSHAKE_TIMEOUT;
            let idle_timed_out = client.player_id.is_some()
                && activity_check_at.duration_since(client.last_message_at) >= CLIENT_IDLE_TIMEOUT;
            if (handshake_timed_out || idle_timed_out)
                && !self.connections.disconnected.contains(&index)
            {
                self.connections.disconnected.push(index);
                if handshake_timed_out {
                    self.metrics.handshake_timeouts =
                        self.metrics.handshake_timeouts.saturating_add(1);
                } else {
                    self.metrics.idle_timeouts = self.metrics.idle_timeouts.saturating_add(1);
                }
            }
        }
        send::send_clients(
            &mut self.connections.clients,
            &mut self.metrics,
            &mut self.connections.disconnected,
            &mut self.connections.first_write_client,
        );
        self.metrics.oldest_output_age = self
            .connections
            .clients
            .iter()
            .filter_map(|client| client.output_frames.front().map(|(_, at)| at.elapsed()))
            .max()
            .unwrap_or_default();
        self.connections.disconnected.sort_unstable();
        self.connections.disconnected.dedup();
        for index in self.connections.disconnected.drain(..).rev() {
            if index < self.connections.clients.len() {
                let client = self.connections.clients.swap_remove(index);
                if let (Some(player_id), Some(profile_id)) = (client.player_id, client.profile_id) {
                    self.connections.client_indices_by_player.remove(&player_id);
                    // Release the active claim immediately, but reject a rejoin
                    // until Leave has applied all preceding queued inputs.
                    if let Some(player) = self.game.world.player(player_id) {
                        if let Err(error) = self.operations.player_store.update_from_player(player)
                        {
                            eprintln!("could not capture disconnect state: {error}");
                        }
                    }
                    self.game.pending_departures.insert(profile_id);
                    self.operations.player_store.release(player_id);
                    self.game
                        .pending_world_inputs
                        .push((WorldInput::Leave(player_id), None));
                    println!("player {player_id} disconnected bytes_in={} bytes_out={} decoded_frames={} queued_frames={} encoding_us={}", client.bytes_in, client.bytes_out, client.frames_in, client.frames_out, client.encoding_us);
                }
                if index < self.connections.clients.len() {
                    if let Some(moved_player_id) = self.connections.clients[index].player_id {
                        self.connections
                            .client_indices_by_player
                            .insert(moved_player_id, index);
                    }
                }
            }
        }
        self.metrics.network_io.record(network_started.elapsed());
        self.metrics.max_pending_inputs = self
            .metrics
            .max_pending_inputs
            .max(self.game.pending_world_inputs.len());

        drop(network_phase);
        Ok(())
    }
}
