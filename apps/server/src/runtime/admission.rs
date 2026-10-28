use super::*;
use crate::{
    connection::{queue_close_response, queue_message, CloseReason},
    world_inputs::WorldInput,
    CHAT_COOLDOWN, GLOBAL_CHAT_COOLDOWN, MAX_PENDING_WORLD_INPUTS, SHOT_COOLDOWN,
};
use minimium_protocol::{ClientMessage, ServerErrorCode, ServerMessage};

impl ServerRuntime {
    pub(super) fn admit_messages(&mut self) {
        for (index, message, decoded_at) in self.connections.messages.drain(..) {
            if index >= self.connections.clients.len()
                || self.connections.disconnected.contains(&index)
                || self.connections.clients[index].close_deadline().is_some()
            {
                continue;
            }
            let player_id = self.connections.clients[index].player_id();
            match message {
                ClientMessage::Identify(profile_id) if player_id.is_none() => {
                    if self.operations.player_store.active_count() >= self.max_players
                        || self.game.pending_world_inputs.len() + self.connections.clients.len()
                            >= MAX_PENDING_WORLD_INPUTS
                    {
                        if !queue_close_response(
                            &mut self.connections.clients[index],
                            &mut self.metrics,
                            ServerMessage::Error(ServerErrorCode::ServerFull),
                            Instant::now(),
                        ) {
                            self.connections.disconnected.push(index);
                        }
                        continue;
                    }
                    if self.game.pending_departures.contains(&profile_id) {
                        if !queue_close_response(
                            &mut self.connections.clients[index],
                            &mut self.metrics,
                            ServerMessage::Error(ServerErrorCode::Busy),
                            Instant::now(),
                        ) {
                            self.connections.disconnected.push(index);
                        }
                        continue;
                    }
                    let player = match self.operations.player_store.claim(
                        profile_id,
                        &mut self.operations.next_player,
                        self.map.spawn(),
                    ) {
                        Ok(player) => player,
                        Err(error) => {
                            eprintln!("could not claim player profile: {error}");
                            if !queue_close_response(
                                &mut self.connections.clients[index],
                                &mut self.metrics,
                                ServerMessage::Error(ServerErrorCode::Busy),
                                Instant::now(),
                            ) {
                                self.connections.disconnected.push(index);
                            }
                            continue;
                        }
                    };
                    self.connections.clients[index].activate(player.id, profile_id);
                    self.connections
                        .client_indices_by_player
                        .insert(player.id, index);
                    self.game.pending_world_inputs.push((
                        WorldInput::Join(
                            player.id,
                            player.name.as_str().to_owned(),
                            player.position,
                            player.health,
                        ),
                        Some(decoded_at),
                    ));
                    if !queue_message(
                        &mut self.connections.clients[index],
                        &mut self.metrics,
                        ServerMessage::Welcome {
                            id: player.id,
                            map_fingerprint: self.map.fingerprint(),
                            health: player.health,
                        },
                    ) {
                        self.metrics.dropped_messages =
                            self.metrics.dropped_messages.saturating_add(1);
                        self.connections.clients[index].disconnect_reason =
                            Some(CloseReason::Backpressure);
                        self.connections.disconnected.push(index);
                    } else {
                        println!("player {} connected", player.id);
                    }
                }
                ClientMessage::Identify(_) => {
                    self.connections.clients[index].disconnect_reason =
                        Some(CloseReason::InvalidSequence);
                    self.connections.disconnected.push(index);
                }
                ClientMessage::Ping if player_id.is_some() => {
                    if !queue_message(
                        &mut self.connections.clients[index],
                        &mut self.metrics,
                        ServerMessage::Pong,
                    ) {
                        self.metrics.dropped_messages =
                            self.metrics.dropped_messages.saturating_add(1);
                    }
                }
                ClientMessage::SetName(name) if player_id.is_some() => {
                    if self.game.pending_world_inputs.len() + self.connections.clients.len()
                        < MAX_PENDING_WORLD_INPUTS
                    {
                        self.game.pending_world_inputs.push((
                            WorldInput::Rename(player_id.unwrap(), name),
                            Some(decoded_at),
                        ));
                        if !queue_message(
                            &mut self.connections.clients[index],
                            &mut self.metrics,
                            ServerMessage::Accepted,
                        ) {
                            self.metrics.dropped_messages =
                                self.metrics.dropped_messages.saturating_add(1);
                        }
                    } else {
                        self.metrics.rejected_inputs =
                            self.metrics.rejected_inputs.saturating_add(1);
                        let _ = queue_message(
                            &mut self.connections.clients[index],
                            &mut self.metrics,
                            ServerMessage::Error(ServerErrorCode::Busy),
                        );
                    }
                }
                ClientMessage::Move { direction, jump } if player_id.is_some() => {
                    if self.game.pending_world_inputs.len() + self.connections.clients.len()
                        < MAX_PENDING_WORLD_INPUTS
                    {
                        self.game.pending_world_inputs.push((
                            WorldInput::Move(
                                player_id.unwrap(),
                                CharacterInput { direction, jump },
                            ),
                            Some(decoded_at),
                        ));
                    } else {
                        self.metrics.rejected_inputs =
                            self.metrics.rejected_inputs.saturating_add(1);
                        let _ = queue_message(
                            &mut self.connections.clients[index],
                            &mut self.metrics,
                            ServerMessage::Error(ServerErrorCode::Busy),
                        );
                    }
                }
                ClientMessage::Fire { direction } if player_id.is_some() => {
                    let now = Instant::now();
                    if self.connections.clients[index]
                        .last_fire_at
                        .is_some_and(|last_fire| {
                            now.saturating_duration_since(last_fire) < SHOT_COOLDOWN
                        })
                    {
                        self.metrics.rejected_inputs =
                            self.metrics.rejected_inputs.saturating_add(1);
                        if !queue_message(
                            &mut self.connections.clients[index],
                            &mut self.metrics,
                            ServerMessage::Error(ServerErrorCode::RateLimit),
                        ) {
                            self.metrics.dropped_messages =
                                self.metrics.dropped_messages.saturating_add(1);
                        }
                    } else if self.game.pending_world_inputs.len() + self.connections.clients.len()
                        < MAX_PENDING_WORLD_INPUTS
                    {
                        self.connections.clients[index].last_fire_at = Some(now);
                        self.game.pending_world_inputs.push((
                            WorldInput::Fire(player_id.unwrap(), direction),
                            Some(decoded_at),
                        ));
                    } else {
                        self.metrics.rejected_inputs =
                            self.metrics.rejected_inputs.saturating_add(1);
                        if !queue_message(
                            &mut self.connections.clients[index],
                            &mut self.metrics,
                            ServerMessage::Error(ServerErrorCode::Busy),
                        ) {
                            self.metrics.dropped_messages =
                                self.metrics.dropped_messages.saturating_add(1);
                        }
                    }
                }
                ClientMessage::Chat(text) if player_id.is_some() => {
                    let now = Instant::now();
                    let rate_limited =
                        self.connections.clients[index]
                            .last_chat_at
                            .is_some_and(|last_chat| {
                                now.saturating_duration_since(last_chat) < CHAT_COOLDOWN
                            })
                            || self.game.last_global_chat_at.is_some_and(|last_chat| {
                                now.saturating_duration_since(last_chat) < GLOBAL_CHAT_COOLDOWN
                            });
                    if rate_limited {
                        self.metrics.rejected_inputs =
                            self.metrics.rejected_inputs.saturating_add(1);
                        if !queue_message(
                            &mut self.connections.clients[index],
                            &mut self.metrics,
                            ServerMessage::Error(ServerErrorCode::RateLimit),
                        ) {
                            self.metrics.dropped_messages =
                                self.metrics.dropped_messages.saturating_add(1);
                        }
                    } else if self.game.pending_world_inputs.len() + self.connections.clients.len()
                        < MAX_PENDING_WORLD_INPUTS
                    {
                        self.connections.clients[index].last_chat_at = Some(now);
                        self.game.last_global_chat_at = Some(now);
                        self.game
                            .pending_world_inputs
                            .push((WorldInput::Chat(player_id.unwrap(), text), Some(decoded_at)));
                    } else {
                        self.metrics.rejected_inputs =
                            self.metrics.rejected_inputs.saturating_add(1);
                        if !queue_message(
                            &mut self.connections.clients[index],
                            &mut self.metrics,
                            ServerMessage::Error(ServerErrorCode::Busy),
                        ) {
                            self.metrics.dropped_messages =
                                self.metrics.dropped_messages.saturating_add(1);
                        }
                    }
                }
                ClientMessage::Quit if player_id.is_some() => {
                    if !queue_close_response(
                        &mut self.connections.clients[index],
                        &mut self.metrics,
                        ServerMessage::Bye,
                        Instant::now(),
                    ) {
                        self.connections.disconnected.push(index);
                    }
                }
                _ => {
                    self.connections.clients[index].disconnect_reason =
                        Some(CloseReason::InvalidSequence);
                    self.connections.disconnected.push(index);
                }
            }
        }
    }
}
