use super::*;
use crate::{
    connection::{queue_close_response, queue_message, CloseReason},
    world_inputs::WorldInput,
    CHAT_COOLDOWN, GLOBAL_CHAT_COOLDOWN, MAX_PENDING_WORLD_INPUTS, SHOT_COOLDOWN,
};
use minimium_protocol::{ClientMessage, ClientWorldInput, ServerErrorCode, ServerMessage};

impl ServerRuntime {
    pub(super) fn admit_messages(&mut self) {
        let mut messages = std::mem::take(&mut self.connections.messages);
        for (index, message, decoded_at) in messages.drain(..) {
            if index >= self.connections.clients.len()
                || self.connections.disconnected.contains(&index)
                || self.connections.clients[index].close_deadline().is_some()
            {
                continue;
            }
            let player_id = self.connections.clients[index].player_id();
            let message = match message {
                ClientMessage::WorldInput {
                    context,
                    input: ClientWorldInput::UsePortal,
                } => {
                    let Some(player) = player_id else {
                        self.connections.clients[index].disconnect_reason =
                            Some(CloseReason::InvalidSequence);
                        self.connections.disconnected.push(index);
                        continue;
                    };
                    if self
                        .game
                        .worlds
                        .world_input_context_is_current(player, context)
                        && self.game.pending_world_inputs.len() + self.connections.clients.len()
                            < MAX_PENDING_WORLD_INPUTS
                    {
                        self.game
                            .pending_world_inputs
                            .push((WorldInput::UsePortal(player, context), Some(decoded_at)));
                    } else {
                        self.metrics.rejected_inputs =
                            self.metrics.rejected_inputs.saturating_add(1);
                    }
                    continue;
                }
                ClientMessage::WorldInput {
                    context,
                    input: ClientWorldInput::MapTeleport { direction },
                } => {
                    if let Some(player) = player_id {
                        if self
                            .game
                            .worlds
                            .world_input_context_is_current(player, context)
                            && self.game.pending_world_inputs.len() + self.connections.clients.len()
                                < MAX_PENDING_WORLD_INPUTS
                        {
                            self.game.pending_world_inputs.push((
                                WorldInput::MapTeleport(player, context, direction),
                                Some(decoded_at),
                            ));
                        } else {
                            self.metrics.rejected_inputs =
                                self.metrics.rejected_inputs.saturating_add(1);
                        }
                    } else {
                        self.connections.clients[index].disconnect_reason =
                            Some(CloseReason::InvalidSequence);
                        self.connections.disconnected.push(index);
                    }
                    continue;
                }
                ClientMessage::TransitionReady {
                    transition_id,
                    target_context,
                } => {
                    let accepted = player_id.is_some_and(|player| {
                        self.game
                            .worlds
                            .mark_transfer_ready(player, transition_id, target_context)
                    });
                    if !accepted {
                        self.metrics.rejected_inputs =
                            self.metrics.rejected_inputs.saturating_add(1);
                    }
                    continue;
                }
                ClientMessage::CancelTransition(transition_id) => {
                    if let Some(player) = player_id {
                        if self.game.worlds.cancel_transfer(player, transition_id) {
                            if let Some(context) = self.game.worlds.context_for_player(player) {
                                let _ = queue_message(
                                    &mut self.connections.clients[index],
                                    &mut self.metrics,
                                    ServerMessage::TransitionCancelled {
                                        transition_id,
                                        context,
                                    },
                                );
                            }
                        }
                    }
                    continue;
                }
                ClientMessage::WorldInput { context, input } => {
                    let Some(player) = player_id else {
                        self.connections.clients[index].disconnect_reason =
                            Some(CloseReason::InvalidSequence);
                        self.connections.disconnected.push(index);
                        continue;
                    };
                    if !self
                        .game
                        .worlds
                        .world_input_context_is_current(player, context)
                    {
                        self.metrics.rejected_inputs =
                            self.metrics.rejected_inputs.saturating_add(1);
                        continue;
                    }
                    match input {
                        ClientWorldInput::SetName(name) => ClientMessage::SetName(name),
                        ClientWorldInput::Move { direction, jump } => {
                            ClientMessage::Move { direction, jump }
                        }
                        ClientWorldInput::Fire { direction } => ClientMessage::Fire { direction },
                        ClientWorldInput::Chat(text) => ClientMessage::Chat(text),
                        ClientWorldInput::UsePortal => unreachable!("portal branch handled above"),
                        ClientWorldInput::MapTeleport { .. } => {
                            unreachable!("teleport branch handled above")
                        }
                    }
                }
                other => other,
            };
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
                        self.map.spawn_position_mm(),
                        self.game.worlds.primary_id(),
                        self.game.worlds.primary().world.gravity(),
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
                    let instance = player.location.instance();
                    let context = match self.game.worlds.reserve_membership(instance, player.id) {
                        Ok(context) => context,
                        Err(error) => {
                            eprintln!("could not reserve world membership: {error:?}");
                            self.connections.disconnected.push(index);
                            continue;
                        }
                    };
                    self.game.pending_world_inputs.push((
                        WorldInput::Join {
                            id: player.id,
                            instance,
                            name: player.name.as_str().to_owned(),
                            position: player.position,
                            position_mm: player.location.position(),
                            health: player.health,
                        },
                        Some(decoded_at),
                    ));
                    let now = Instant::now();
                    let (clock_sample, environment) =
                        match self.environment_state_for_instance(instance, now) {
                            Ok(state) => state,
                            Err(error) => {
                                eprintln!("could not sample environment for Welcome: {error}");
                                self.connections.clients[index].disconnect_reason =
                                    Some(CloseReason::InvalidSequence);
                                self.connections.disconnected.push(index);
                                continue;
                            }
                        };
                    let queued = queue_message(
                        &mut self.connections.clients[index],
                        &mut self.metrics,
                        ServerMessage::Welcome {
                            id: player.id,
                            map_fingerprint: self
                                .game
                                .worlds
                                .map_fingerprint_for_instance(instance)
                                .expect("reserved membership references a loaded world"),
                            health: player.health,
                            position: player.position,
                            position_mm: player.location.position(),
                            context,
                            manifest: self
                                .game
                                .worlds
                                .manifest_for_player(player.id)
                                .expect("reserved membership has a manifest"),
                            transition_id: None,
                            clock_sample,
                            environment,
                        },
                    );
                    if !queued {
                        self.metrics.dropped_messages =
                            self.metrics.dropped_messages.saturating_add(1);
                        self.connections.clients[index].disconnect_reason =
                            Some(CloseReason::Backpressure);
                        self.connections.disconnected.push(index);
                    } else {
                        self.connections.clients[index].mark_environment_sent(
                            context,
                            clock_sample.revision(),
                            Instant::now(),
                        );
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
                ClientMessage::TimeProbe {
                    seq,
                    client_send_monotonic_ms,
                } if player_id.is_some() => {
                    self.metrics.clock_probe_requests =
                        self.metrics.clock_probe_requests.saturating_add(1);
                    let now = Instant::now();
                    if !self.connections.clients[index].accept_time_probe(now) {
                        self.metrics.clock_probe_rejected =
                            self.metrics.clock_probe_rejected.saturating_add(1);
                        self.metrics.rejected_inputs =
                            self.metrics.rejected_inputs.saturating_add(1);
                        let may_reply = self.connections.clients[index]
                            .last_time_probe_rejection_at
                            .is_none_or(|last| {
                                now.saturating_duration_since(last) >= Duration::from_secs(1)
                            });
                        if may_reply
                            && queue_message(
                                &mut self.connections.clients[index],
                                &mut self.metrics,
                                ServerMessage::Error(ServerErrorCode::RateLimit),
                            )
                        {
                            self.connections.clients[index].last_time_probe_rejection_at =
                                Some(now);
                        }
                        continue;
                    }
                    let Ok(server_receive_monotonic_ms) = self.monotonic_ms(decoded_at) else {
                        self.metrics.clock_probe_rejected =
                            self.metrics.clock_probe_rejected.saturating_add(1);
                        self.metrics.rejected_inputs =
                            self.metrics.rejected_inputs.saturating_add(1);
                        continue;
                    };
                    let send_at = Instant::now();
                    let Ok(server_send_monotonic_ms) = self.monotonic_ms(send_at) else {
                        self.metrics.clock_probe_rejected =
                            self.metrics.clock_probe_rejected.saturating_add(1);
                        self.metrics.rejected_inputs =
                            self.metrics.rejected_inputs.saturating_add(1);
                        continue;
                    };
                    let Ok(sample) = self.environment_clock.sample(server_send_monotonic_ms) else {
                        self.metrics.clock_probe_rejected =
                            self.metrics.clock_probe_rejected.saturating_add(1);
                        self.metrics.rejected_inputs =
                            self.metrics.rejected_inputs.saturating_add(1);
                        continue;
                    };
                    if queue_message(
                        &mut self.connections.clients[index],
                        &mut self.metrics,
                        ServerMessage::TimeProbeReply {
                            seq,
                            echoed_client_send_monotonic_ms: client_send_monotonic_ms,
                            server_receive_monotonic_ms: server_receive_monotonic_ms.get(),
                            server_send_monotonic_ms: server_send_monotonic_ms.get(),
                            session_nonce: sample.session_nonce(),
                        },
                    ) {
                        self.metrics.clock_probe_accepted =
                            self.metrics.clock_probe_accepted.saturating_add(1);
                    } else {
                        self.metrics.clock_probe_rejected =
                            self.metrics.clock_probe_rejected.saturating_add(1);
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
        self.connections.messages = messages;
    }
}
