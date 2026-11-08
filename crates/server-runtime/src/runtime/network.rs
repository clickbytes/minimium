use super::*;
use crate::{
    accept,
    connection::{mark_disconnected, CloseReason},
    receive, send,
    world_inputs::WorldInput,
};
use minimium_telemetry::allocations::{self, Phase};

impl ServerRuntime {
    pub(super) fn poll_network(&mut self) -> std::io::Result<()> {
        let network_phase = allocations::enter(Phase::Network);
        let network_started = Instant::now();
        let Some(listener) = self.game_listener.as_ref() else {
            return Ok(());
        };
        accept::accept_clients(
            listener,
            &mut self.connections.clients,
            self.max_client_connections,
            self.game.pending_world_inputs.len(),
            &self.transport_logger,
            &self.protocol_logger,
        )?;

        receive::poll_clients(
            &mut self.connections.clients,
            &mut self.metrics,
            &mut self.connections.messages,
            &mut self.connections.disconnected,
        );
        self.admit_messages();
        self.close_eof_clients(Instant::now());
        self.mark_timed_out_clients(Instant::now());
        self.replicate_environment();
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
            .filter_map(Client::oldest_output_age)
            .max()
            .unwrap_or_default();
        self.remove_disconnected_clients();
        self.metrics.network_io.record(network_started.elapsed());
        self.metrics.max_pending_inputs = self
            .metrics
            .max_pending_inputs
            .max(self.game.pending_world_inputs.len());

        drop(network_phase);
        Ok(())
    }
    fn close_eof_clients(&mut self, eof_close_at: Instant) {
        for client in &mut self.connections.clients {
            if client.peer_eof && client.close_deadline().is_none() {
                client.begin_close(eof_close_at, CloseReason::PeerEof);
            }
        }
    }

    fn mark_timed_out_clients(&mut self, activity_check_at: Instant) {
        for (index, client) in self.connections.clients.iter_mut().enumerate() {
            if client.close_deadline().is_some() {
                continue;
            }
            let handshake_timed_out = client.player_id().is_none()
                && client
                    .admission_deadline()
                    .is_some_and(|deadline| activity_check_at >= deadline);
            let idle_timed_out = client.player_id().is_some()
                && activity_check_at.duration_since(client.last_message_at) >= CLIENT_IDLE_TIMEOUT;
            if (handshake_timed_out || idle_timed_out)
                && !self.connections.disconnected.contains(&index)
            {
                if handshake_timed_out {
                    mark_disconnected(
                        index,
                        client,
                        &mut self.connections.disconnected,
                        Some(CloseReason::HandshakeTimeout),
                    );
                    self.metrics.handshake_timeouts =
                        self.metrics.handshake_timeouts.saturating_add(1);
                } else {
                    mark_disconnected(
                        index,
                        client,
                        &mut self.connections.disconnected,
                        Some(CloseReason::IdleTimeout),
                    );
                    self.metrics.idle_timeouts = self.metrics.idle_timeouts.saturating_add(1);
                }
            }
        }
    }

    fn remove_disconnected_clients(&mut self) {
        self.connections.disconnected.sort_unstable();
        self.connections.disconnected.dedup();
        for index in self.connections.disconnected.drain(..).rev() {
            if index < self.connections.clients.len() {
                let client = self.connections.clients.swap_remove(index);
                if let Some(attempt) = client.authentication_attempt_for_cancel() {
                    self.authentication_provider.cancel(attempt);
                }
                if let Some(identity) = client.identity() {
                    let player_id = identity.player;
                    self.game.player_movement_policies.remove(&player_id);
                    let profile_id = identity.principal.profile();
                    self.connections.client_indices_by_player.remove(&player_id);
                    // Release the active claim immediately, but reject a rejoin
                    // until Leave has applied all preceding queued inputs.
                    if let Some(instance_id) = self.game.worlds.instance_for_player(player_id) {
                        if let Some(instance) = self.game.worlds.get(instance_id) {
                            let gravity = instance.world.gravity();
                            if let Some(player) = instance.world.player(player_id) {
                                if let Err(error) = self.operations.player_store.update_from_player(
                                    player,
                                    instance_id,
                                    gravity,
                                ) {
                                    crate::diagnostics::emit(
                                        &self.transport_logger,
                                        minimium_diagnostics::Level::Error,
                                        "server.disconnect_save_failed",
                                        &[("reason", error.to_string())],
                                    );
                                }
                            }
                        }
                    }
                    self.game.pending_departures.insert(profile_id);
                    self.operations.player_store.release(player_id);
                    self.game
                        .pending_world_inputs
                        .push((WorldInput::Leave(player_id), None));
                    crate::diagnostics::emit(
                        &self.transport_logger,
                        minimium_diagnostics::Level::Info,
                        "transport.player_disconnected",
                        &[
                            ("player_id", player_id.to_string()),
                            ("reason", format!("{:?}", client.reason())),
                            ("bytes_in", client.bytes_in.to_string()),
                            ("bytes_out", client.bytes_out.to_string()),
                            ("decoded_frames", client.frames_in.to_string()),
                            ("queued_frames", client.frames_out.to_string()),
                            ("encoding_us", client.encoding_us.to_string()),
                        ],
                    );
                } else {
                    crate::diagnostics::emit(
                        &self.transport_logger,
                        minimium_diagnostics::Level::Info,
                        "transport.unidentified_client_disconnected",
                        &[("reason", format!("{:?}", client.reason()))],
                    );
                }
                if index < self.connections.clients.len() {
                    if let Some(moved_player_id) = self.connections.clients[index].player_id() {
                        self.connections
                            .client_indices_by_player
                            .insert(moved_player_id, index);
                    }
                }
            }
        }
    }
}
