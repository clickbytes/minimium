use crate::{
    connection::{queue_encoded_frame, queue_message, Client},
    message_dispatch::WorldInput,
    metrics::RuntimeMetrics,
    storage::PlayerStore,
};
use minimium_core::{
    identity::PlayerProfileId,
    world::{CharacterInput, World},
};
use minimium_protocol::{encode_server, ServerMessage};
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

const INPUT_IDLE_TIMEOUT: Duration = Duration::from_millis(250);

pub(crate) fn expire_stale_inputs(
    active_inputs: &mut HashMap<u64, CharacterInput>,
    input_last_received: &HashMap<u64, Instant>,
    now: Instant,
) {
    for (id, last_received) in input_last_received {
        if now.duration_since(*last_received) > INPUT_IDLE_TIMEOUT {
            if let Some(input) = active_inputs.get_mut(id) {
                input.direction = [0.0; 2];
            }
        }
    }
}

pub(crate) struct WorldInputsContext<'a> {
    pub(crate) pending_world_inputs: &'a mut Vec<(WorldInput, Option<Instant>)>,
    pub(crate) world: &'a mut World,
    pub(crate) active_inputs: &'a mut HashMap<u64, CharacterInput>,
    pub(crate) input_last_received: &'a mut HashMap<u64, Instant>,
    pub(crate) player_store: &'a mut PlayerStore,
    pub(crate) pending_departures: &'a mut HashSet<PlayerProfileId>,
    pub(crate) client_indices_by_player: &'a HashMap<u64, usize>,
    pub(crate) clients: &'a mut Vec<Client>,
    pub(crate) metrics: &'a mut RuntimeMetrics,
}

pub(crate) fn apply_world_inputs(context: WorldInputsContext<'_>) {
    let WorldInputsContext {
        pending_world_inputs,
        world,
        active_inputs,
        input_last_received,
        player_store,
        pending_departures,
        client_indices_by_player,
        clients,
        metrics,
    } = context;

    for (input, decoded_at) in pending_world_inputs.drain(..) {
        if let Some(decoded_at) = decoded_at {
            metrics
                .input_wait
                .record(Instant::now().saturating_duration_since(decoded_at));
        }
        match input {
            WorldInput::Join(id, name, position, health) => {
                world.join_at_with_health(id, name, position, health);
                active_inputs.insert(id, CharacterInput::default());
                input_last_received.insert(id, Instant::now());
            }
            WorldInput::Leave(id) => {
                if let Some(player) = world.player(id) {
                    if let Err(error) = player_store.update_from_player(player) {
                        eprintln!("could not capture player {} before leave: {error}", id);
                    }
                }
                world.leave(id);
                active_inputs.remove(&id);
                input_last_received.remove(&id);
                if let Some(profile_id) = player_store.profile_by_player.get(&id) {
                    pending_departures.remove(profile_id);
                }
            }
            WorldInput::Move(id, input) => {
                let current = active_inputs.entry(id).or_default();
                current.direction = input.direction;
                current.jump |= input.jump;
                input_last_received.insert(id, Instant::now());
            }
            WorldInput::Fire(id, direction) => {
                metrics.shots_fired = metrics.shots_fired.saturating_add(1);
                let Some(outcome) = world.fire_hitscan(id, direction) else {
                    continue;
                };
                if let Some(target_id) = outcome.target_id {
                    metrics.shots_hit = metrics.shots_hit.saturating_add(1);
                    if outcome.killed {
                        metrics.player_kills = metrics.player_kills.saturating_add(1);
                    }
                    if let Some(target_index) = client_indices_by_player.get(&target_id).copied() {
                        if let Some(health) = world.player_health(target_id) {
                            clients[target_index].pending_health_update = Some(health);
                        }
                    }
                }
                if let Some(shooter_index) = client_indices_by_player.get(&id).copied() {
                    let message = ServerMessage::ShotResult {
                        target_id: outcome.target_id,
                        remaining_health: outcome.remaining_health,
                        killed: outcome.killed,
                    };
                    if !queue_message(&mut clients[shooter_index], metrics, message) {
                        metrics.dropped_messages = metrics.dropped_messages.saturating_add(1);
                    }
                }
            }
            WorldInput::Chat(id, text) => {
                if !player_store.active_players.contains(&id) {
                    continue;
                }
                let Some(sender_name) = player_store.name_for_player(id) else {
                    continue;
                };
                let message = ServerMessage::Chat {
                    sender_id: id,
                    sender_name: sender_name.to_owned(),
                    text,
                };
                let encoding_started = Instant::now();
                let encoded = encode_server(&message);
                metrics.encoding.record(encoding_started.elapsed());
                let Ok(frame) = encoded else {
                    metrics.dropped_messages = metrics.dropped_messages.saturating_add(1);
                    continue;
                };
                for client in &mut *clients {
                    if !queue_encoded_frame(client, &frame, metrics) {
                        metrics.dropped_messages = metrics.dropped_messages.saturating_add(1);
                    }
                }
            }
            WorldInput::Rename(id, name) => {
                world.rename_player(id, name.clone());
                player_store.rename(id, name);
            }
        }
    }
}
