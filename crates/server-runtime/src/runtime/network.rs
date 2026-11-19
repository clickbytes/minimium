use super::*;
use crate::{
    connection::{mark_disconnected, queue_close_response, Client, CloseReason},
    world_inputs::WorldInput,
};
use minimium_telemetry::allocations::{self, Phase};
use minimium_transport::{GameDisconnectReason, GameTransportEvent, TransportEvent};

impl ServerRuntime {
    pub(super) fn poll_network(&mut self) -> std::io::Result<()> {
        let _network_phase = allocations::enter(Phase::Network);
        let network_started = Instant::now();
        let now = self.now;
        let events = self
            .operations
            .transport
            .poll(now)
            .map_err(std::io::Error::other)?;
        let transport_metrics = self.operations.transport.metrics();
        self.metrics.bytes_in = transport_metrics.bytes_in;
        self.metrics.bytes_out = transport_metrics.bytes_out;
        self.metrics.frames_in = transport_metrics.frames_in;
        self.metrics.frames_out = transport_metrics.frames_out;
        self.metrics.max_client_input_bytes = transport_metrics.max_client_input_bytes;
        self.metrics.max_client_output_bytes = transport_metrics.max_client_output_bytes;
        let sender = self.operations.transport.game_output();
        for event in events {
            match event {
                TransportEvent::Game(GameTransportEvent::Connected {
                    connection_id, at, ..
                }) => {
                    let rejection = if self.connections.clients.len() >= self.max_client_connections
                        || self.game.pending_world_inputs.len() + self.connections.clients.len()
                            >= MAX_PENDING_WORLD_INPUTS
                    {
                        Some(minimium_protocol::ServerErrorCode::ServerFull)
                    } else if self
                        .connections
                        .clients
                        .iter()
                        .filter(|client| client.player_id().is_none())
                        .count()
                        >= MAX_PENDING_HANDSHAKES
                    {
                        Some(minimium_protocol::ServerErrorCode::Busy)
                    } else {
                        None
                    };
                    self.connections.clients.push(Client::new(
                        connection_id,
                        sender.clone(),
                        at,
                        Some(self.protocol_logger.clone()),
                    ));
                    let index = self.connections.clients.len() - 1;
                    if let Some(code) = rejection {
                        let queued = queue_close_response(
                            &mut self.connections.clients[index],
                            &mut self.metrics,
                            minimium_protocol::ServerMessage::Error(code),
                            at,
                        );
                        if !queued {
                            self.connections.disconnected.push(index);
                        }
                    }
                }
                TransportEvent::Game(GameTransportEvent::Message {
                    connection_id,
                    message,
                    at,
                }) => {
                    if let Some(index) = self
                        .connections
                        .clients
                        .iter()
                        .position(|client| client.connection_id() == connection_id)
                    {
                        if self.connections.clients[index].close_deadline().is_none() {
                            self.connections.clients[index].last_message_at = at;
                            self.connections.messages.push((index, message, at));
                        }
                    }
                }
                TransportEvent::Game(GameTransportEvent::Closing {
                    connection_id,
                    reason,
                    at,
                }) => {
                    if let Some(client) = self
                        .connections
                        .clients
                        .iter_mut()
                        .find(|client| client.connection_id() == connection_id)
                    {
                        let reason = match reason {
                            GameDisconnectReason::Rejected => CloseReason::Rejected,
                            GameDisconnectReason::Quit => CloseReason::Quit,
                            _ => CloseReason::InvalidFrame,
                        };
                        client.begin_close(at, reason);
                    }
                }
                TransportEvent::Game(GameTransportEvent::SocketSetupFailed {
                    operation,
                    error_kind,
                }) => {
                    crate::diagnostics::emit(
                        &self.transport_logger,
                        minimium_diagnostics::Level::Warn,
                        "transport.client_socket_setup_failed",
                        &[
                            ("operation", format!("{operation:?}").to_lowercase()),
                            ("error_kind", format!("{error_kind:?}")),
                        ],
                    );
                }
                TransportEvent::Game(GameTransportEvent::Disconnected {
                    connection_id,
                    reason,
                    stats,
                }) => {
                    if let Some(index) = self
                        .connections
                        .clients
                        .iter()
                        .position(|client| client.connection_id() == connection_id)
                    {
                        self.connections.clients[index].network_stats = stats;
                        let reason = match reason {
                            GameDisconnectReason::PeerEof => CloseReason::PeerEof,
                            GameDisconnectReason::HandshakeTimeout => CloseReason::HandshakeTimeout,
                            GameDisconnectReason::IdleTimeout => CloseReason::IdleTimeout,
                            GameDisconnectReason::InvalidFrame => CloseReason::InvalidFrame,
                            GameDisconnectReason::ReadError => CloseReason::ReadError,
                            GameDisconnectReason::WriteError => CloseReason::WriteError,
                            GameDisconnectReason::Backpressure => CloseReason::Backpressure,
                            GameDisconnectReason::Closed => self.connections.clients[index]
                                .reason()
                                .unwrap_or(CloseReason::PeerEof),
                            GameDisconnectReason::Rejected => CloseReason::Rejected,
                            GameDisconnectReason::Quit => CloseReason::Quit,
                        };
                        self.connections.clients[index].disconnect_reason = Some(reason);
                        mark_disconnected(
                            index,
                            &mut self.connections.clients[index],
                            &mut self.connections.disconnected,
                            None,
                        );
                    }
                }
                TransportEvent::Admin(minimium_transport::AdminTransportEvent::Request {
                    id,
                    line,
                    ..
                }) => {
                    if self.connections.admin_requests.len() < 64 {
                        self.connections.admin_requests.push_back((id, line));
                    } else {
                        if let Err(error) = self.operations.transport.respond_admin(
                            id,
                            minimium_transport::AdminResponse {
                                success: false,
                                error_code: Some("server.command.queue_full".to_owned()),
                                key: None,
                                text: "command queue full".to_owned(),
                                usage: None,
                            },
                        ) {
                            crate::diagnostics::emit(
                                &self.server_logger,
                                minimium_diagnostics::Level::Debug,
                                "server.admin_response_dropped",
                                &[("reason", error.to_string())],
                            );
                        }
                    }
                }
            }
        }
        self.admit_messages();
        self.mark_timed_out_clients(self.now);
        for client in &mut self.connections.clients {
            if client.close_deadline().is_none() {
                if let Some(health) = client.pending_health_update {
                    if crate::connection::queue_message(
                        client,
                        &mut self.metrics,
                        minimium_protocol::ServerMessage::HealthUpdate(health),
                    ) {
                        client.pending_health_update = None;
                    } else {
                        self.metrics.dropped_messages =
                            self.metrics.dropped_messages.saturating_add(1);
                    }
                }
            }
        }
        for event in self.operations.transport.flush(self.now) {
            if let TransportEvent::Game(GameTransportEvent::Disconnected {
                connection_id,
                reason,
                stats,
            }) = event
            {
                if let Some(index) = self
                    .connections
                    .clients
                    .iter()
                    .position(|client| client.connection_id() == connection_id)
                {
                    self.connections.clients[index].network_stats = stats;
                    let close_reason = match reason {
                        GameDisconnectReason::PeerEof => CloseReason::PeerEof,
                        GameDisconnectReason::HandshakeTimeout => CloseReason::HandshakeTimeout,
                        GameDisconnectReason::IdleTimeout => CloseReason::IdleTimeout,
                        GameDisconnectReason::InvalidFrame => CloseReason::InvalidFrame,
                        GameDisconnectReason::ReadError => CloseReason::ReadError,
                        GameDisconnectReason::WriteError => CloseReason::WriteError,
                        GameDisconnectReason::Backpressure => CloseReason::Backpressure,
                        GameDisconnectReason::Closed => self.connections.clients[index]
                            .reason()
                            .unwrap_or(CloseReason::PeerEof),
                        GameDisconnectReason::Rejected => CloseReason::Rejected,
                        GameDisconnectReason::Quit => CloseReason::Quit,
                    };
                    self.connections.clients[index].disconnect_reason = Some(close_reason);
                    self.connections.disconnected.push(index);
                }
            }
        }
        let transport_metrics = self.operations.transport.metrics();
        self.metrics.bytes_in = transport_metrics.bytes_in;
        self.metrics.bytes_out = transport_metrics.bytes_out;
        self.metrics.frames_in = transport_metrics.frames_in;
        self.metrics.frames_out = transport_metrics.frames_out;
        self.metrics.max_client_input_bytes = self
            .metrics
            .max_client_input_bytes
            .max(transport_metrics.max_client_input_bytes);
        self.metrics.max_client_output_bytes = self
            .metrics
            .max_client_output_bytes
            .max(transport_metrics.max_client_output_bytes);
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
        Ok(())
    }

    fn mark_timed_out_clients(&mut self, at: Instant) {
        for (index, client) in self.connections.clients.iter_mut().enumerate() {
            if client.close_deadline().is_some() {
                continue;
            }
            let handshake_timed_out = client.player_id().is_none()
                && client
                    .admission_deadline()
                    .is_some_and(|deadline| at >= deadline);
            let idle_timed_out = client.player_id().is_some()
                && at.duration_since(client.last_message_at) >= CLIENT_IDLE_TIMEOUT;
            if handshake_timed_out || idle_timed_out {
                let reason = if handshake_timed_out {
                    self.metrics.handshake_timeouts =
                        self.metrics.handshake_timeouts.saturating_add(1);
                    CloseReason::HandshakeTimeout
                } else {
                    self.metrics.idle_timeouts = self.metrics.idle_timeouts.saturating_add(1);
                    CloseReason::IdleTimeout
                };
                mark_disconnected(
                    index,
                    client,
                    &mut self.connections.disconnected,
                    Some(reason),
                );
                client.transport.close(client.connection_id());
            }
        }
    }

    fn remove_disconnected_clients(&mut self) {
        self.connections.disconnected.sort_unstable();
        self.connections.disconnected.dedup();
        for index in self.connections.disconnected.drain(..).rev() {
            if index >= self.connections.clients.len() {
                continue;
            }
            let client = self.connections.clients.swap_remove(index);
            client.transport.close(client.connection_id());
            if let Some(attempt) = client.authentication_attempt_for_cancel() {
                self.authentication_provider.cancel(attempt);
            }
            if let Some(identity) = client.identity() {
                let player_id = identity.player;
                self.equipment.remove_actor(player_id);
                self.game.player_movement_policies.remove(&player_id);
                let profile_id = identity.principal.profile();
                self.connections.client_indices_by_player.remove(&player_id);
                if let Some(instance_id) = self.game.worlds.instance_for_player(player_id) {
                    if let Some(instance) = self.game.worlds.get(instance_id) {
                        if let Some(player) = instance.world.player(player_id) {
                            if let Err(error) = self.operations.player_store.update_from_player(
                                player,
                                instance_id,
                                instance.world.gravity(),
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
                        ("bytes_in", client.network_stats.bytes_in.to_string()),
                        ("bytes_out", client.network_stats.bytes_out.to_string()),
                        ("decoded_frames", client.network_stats.frames_in.to_string()),
                        ("queued_frames", client.network_stats.frames_out.to_string()),
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
