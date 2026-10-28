use crate::{
    connection::{queue_close_response, queue_message, Client},
    metrics::RuntimeMetrics,
    storage::PlayerStore,
    world_inputs::WorldInput,
    CHAT_COOLDOWN, GLOBAL_CHAT_COOLDOWN, MAX_PENDING_WORLD_INPUTS, SHOT_COOLDOWN,
};
use minimium_core::ids::PlayerId;
use minimium_core::{
    identity::PlayerProfileId,
    world::{CharacterInput, WorldMap},
};
use minimium_protocol::{
    ClientMessage, ServerMessage, ERROR_BUSY, ERROR_RATE_LIMIT, ERROR_SERVER_FULL,
};
use std::{
    collections::{HashMap, HashSet},
    time::Instant,
};

pub(crate) struct DispatchContext<'a> {
    pub(crate) clients: &'a mut Vec<Client>,
    pub(crate) disconnected: &'a mut Vec<usize>,
    pub(crate) player_store: &'a mut PlayerStore,
    pub(crate) max_players: usize,
    pub(crate) pending_world_inputs: &'a mut Vec<(WorldInput, Option<Instant>)>,
    pub(crate) pending_departures: &'a HashSet<PlayerProfileId>,
    pub(crate) next_player: &'a mut u64,
    pub(crate) map: &'a WorldMap,
    pub(crate) client_indices_by_player: &'a mut HashMap<PlayerId, usize>,
    pub(crate) metrics: &'a mut RuntimeMetrics,
    pub(crate) last_global_chat_at: &'a mut Option<Instant>,
    pub(crate) messages: &'a mut Vec<(usize, ClientMessage, Instant)>,
}

pub(crate) fn dispatch_messages(context: DispatchContext<'_>) {
    let DispatchContext {
        clients,
        disconnected,
        player_store,
        max_players,
        pending_world_inputs,
        pending_departures,
        next_player,
        map,
        client_indices_by_player,
        metrics,
        last_global_chat_at,
        messages,
    } = context;

    for (index, message, decoded_at) in messages.drain(..) {
        if index >= clients.len()
            || disconnected.contains(&index)
            || clients[index].close_deadline.is_some()
        {
            continue;
        }
        let player_id = clients[index].player_id;
        match message {
            ClientMessage::Identify(bytes) if player_id.is_none() => {
                if player_store.active_players.len() >= max_players
                    || pending_world_inputs.len() + clients.len() >= MAX_PENDING_WORLD_INPUTS
                {
                    if !queue_close_response(
                        &mut clients[index],
                        metrics,
                        ServerMessage::Error(ERROR_SERVER_FULL),
                        Instant::now(),
                    ) {
                        disconnected.push(index);
                    }
                    continue;
                }
                let profile_id = PlayerProfileId::from_bytes(bytes);
                if pending_departures.contains(&profile_id) {
                    if !queue_close_response(
                        &mut clients[index],
                        metrics,
                        ServerMessage::Error(ERROR_BUSY),
                        Instant::now(),
                    ) {
                        disconnected.push(index);
                    }
                    continue;
                }
                let player = match player_store.claim(profile_id, next_player, map.spawn) {
                    Ok(player) => player,
                    Err(error) => {
                        eprintln!("could not claim player profile: {error}");
                        if !queue_close_response(
                            &mut clients[index],
                            metrics,
                            ServerMessage::Error(ERROR_BUSY),
                            Instant::now(),
                        ) {
                            disconnected.push(index);
                        }
                        continue;
                    }
                };
                clients[index].player_id = Some(player.id);
                clients[index].profile_id = Some(profile_id);
                client_indices_by_player.insert(player.id, index);
                pending_world_inputs.push((
                    WorldInput::Join(
                        player.id,
                        player.name.as_str().to_owned(),
                        player.position,
                        player.health,
                    ),
                    Some(decoded_at),
                ));
                if !queue_message(
                    &mut clients[index],
                    metrics,
                    ServerMessage::Welcome {
                        id: player.id.get(),
                        map_fingerprint: map.fingerprint(),
                        health: player.health,
                    },
                ) {
                    metrics.dropped_messages = metrics.dropped_messages.saturating_add(1);
                    disconnected.push(index);
                } else {
                    println!("player {} connected", player.id);
                }
            }
            ClientMessage::Identify(_) => disconnected.push(index),
            ClientMessage::Ping if player_id.is_some() => {
                if !queue_message(&mut clients[index], metrics, ServerMessage::Pong) {
                    metrics.dropped_messages = metrics.dropped_messages.saturating_add(1);
                }
            }
            ClientMessage::SetName(name) if player_id.is_some() => {
                if pending_world_inputs.len() + clients.len() < MAX_PENDING_WORLD_INPUTS {
                    pending_world_inputs.push((
                        WorldInput::Rename(player_id.unwrap(), name),
                        Some(decoded_at),
                    ));
                    if !queue_message(&mut clients[index], metrics, ServerMessage::Accepted) {
                        metrics.dropped_messages = metrics.dropped_messages.saturating_add(1);
                    }
                } else {
                    metrics.rejected_inputs = metrics.rejected_inputs.saturating_add(1);
                    let _ = queue_message(
                        &mut clients[index],
                        metrics,
                        ServerMessage::Error(ERROR_BUSY),
                    );
                }
            }
            ClientMessage::Move { direction, jump } if player_id.is_some() => {
                if pending_world_inputs.len() + clients.len() < MAX_PENDING_WORLD_INPUTS {
                    pending_world_inputs.push((
                        WorldInput::Move(player_id.unwrap(), CharacterInput { direction, jump }),
                        Some(decoded_at),
                    ));
                } else {
                    metrics.rejected_inputs = metrics.rejected_inputs.saturating_add(1);
                    let _ = queue_message(
                        &mut clients[index],
                        metrics,
                        ServerMessage::Error(ERROR_BUSY),
                    );
                }
            }
            ClientMessage::Fire { direction } if player_id.is_some() => {
                let now = Instant::now();
                if clients[index].last_fire_at.is_some_and(|last_fire| {
                    now.saturating_duration_since(last_fire) < SHOT_COOLDOWN
                }) {
                    metrics.rejected_inputs = metrics.rejected_inputs.saturating_add(1);
                    if !queue_message(
                        &mut clients[index],
                        metrics,
                        ServerMessage::Error(ERROR_RATE_LIMIT),
                    ) {
                        metrics.dropped_messages = metrics.dropped_messages.saturating_add(1);
                    }
                } else if pending_world_inputs.len() + clients.len() < MAX_PENDING_WORLD_INPUTS {
                    clients[index].last_fire_at = Some(now);
                    pending_world_inputs.push((
                        WorldInput::Fire(player_id.unwrap(), direction),
                        Some(decoded_at),
                    ));
                } else {
                    metrics.rejected_inputs = metrics.rejected_inputs.saturating_add(1);
                    if !queue_message(
                        &mut clients[index],
                        metrics,
                        ServerMessage::Error(ERROR_BUSY),
                    ) {
                        metrics.dropped_messages = metrics.dropped_messages.saturating_add(1);
                    }
                }
            }
            ClientMessage::Chat(text) if player_id.is_some() => {
                let now = Instant::now();
                let rate_limited = clients[index].last_chat_at.is_some_and(|last_chat| {
                    now.saturating_duration_since(last_chat) < CHAT_COOLDOWN
                }) || last_global_chat_at.is_some_and(|last_chat| {
                    now.saturating_duration_since(last_chat) < GLOBAL_CHAT_COOLDOWN
                });
                if rate_limited {
                    metrics.rejected_inputs = metrics.rejected_inputs.saturating_add(1);
                    if !queue_message(
                        &mut clients[index],
                        metrics,
                        ServerMessage::Error(ERROR_RATE_LIMIT),
                    ) {
                        metrics.dropped_messages = metrics.dropped_messages.saturating_add(1);
                    }
                } else if pending_world_inputs.len() + clients.len() < MAX_PENDING_WORLD_INPUTS {
                    clients[index].last_chat_at = Some(now);
                    *last_global_chat_at = Some(now);
                    pending_world_inputs
                        .push((WorldInput::Chat(player_id.unwrap(), text), Some(decoded_at)));
                } else {
                    metrics.rejected_inputs = metrics.rejected_inputs.saturating_add(1);
                    if !queue_message(
                        &mut clients[index],
                        metrics,
                        ServerMessage::Error(ERROR_BUSY),
                    ) {
                        metrics.dropped_messages = metrics.dropped_messages.saturating_add(1);
                    }
                }
            }
            ClientMessage::Quit if player_id.is_some() => {
                if !queue_close_response(
                    &mut clients[index],
                    metrics,
                    ServerMessage::Bye,
                    Instant::now(),
                ) {
                    disconnected.push(index);
                }
            }
            _ => disconnected.push(index),
        }
    }
}
