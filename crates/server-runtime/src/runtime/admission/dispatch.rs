use super::{actions, identity, routing::RoutingOutcome, technical, world_control};
use crate::{
    connection::{mark_disconnected, CloseReason},
    runtime::ServerRuntime,
};
use minimium_core::ids::PlayerId;
use minimium_protocol::ClientMessage;
use std::time::Instant;

pub(super) fn dispatch_message(
    runtime: &mut ServerRuntime,
    index: usize,
    message: ClientMessage,
    decoded_at: Instant,
) {
    let player_id = runtime.connections.clients[index].player_id();
    // Routing preserves wrapper conversion and pre-dispatch effects; only continued messages reach admission below.
    let message = match identity::negotiate_content(runtime, index, message, player_id) {
        RoutingOutcome::Handled => return,
        RoutingOutcome::ContinueWith(message) => message,
    };
    let message = match world_control::route_message(runtime, index, message, decoded_at, player_id)
    {
        RoutingOutcome::Handled => return,
        RoutingOutcome::ContinueWith(message) => message,
    };
    let message = match runtime.route_message(index, message, player_id) {
        RoutingOutcome::Handled => return,
        RoutingOutcome::ContinueWith(message) => message,
    };
    let message = match technical::handle_message(runtime, index, message, decoded_at, player_id) {
        RoutingOutcome::Handled => return,
        RoutingOutcome::ContinueWith(message) => message,
    };
    handle_admitted_message(runtime, index, message, decoded_at, player_id);
}

fn handle_admitted_message(
    runtime: &mut ServerRuntime,
    index: usize,
    message: ClientMessage,
    decoded_at: Instant,
    player_id: Option<PlayerId>,
) {
    match message {
        ClientMessage::Identify(profile_id) if player_id.is_none() => {
            identity::activate_profile(runtime, index, profile_id, decoded_at);
        }
        ClientMessage::Identify(_) => {
            disconnect_invalid_sequence(runtime, index);
        }
        ClientMessage::SetName(name) => {
            if let Some(player) = player_id {
                actions::movement::admit_name(runtime, index, player, name, decoded_at);
            } else {
                disconnect_invalid_sequence(runtime, index);
            }
        }
        ClientMessage::Motion(input) => {
            if let Some(player) = player_id {
                actions::movement::admit_motion(runtime, index, player, input, decoded_at);
            } else {
                disconnect_invalid_sequence(runtime, index);
            }
        }
        ClientMessage::Fire { direction } => {
            if let Some(player) = player_id {
                actions::combat::admit_fire(runtime, index, player, direction, decoded_at);
            } else {
                disconnect_invalid_sequence(runtime, index);
            }
        }
        ClientMessage::Chat(text) => {
            if let Some(player) = player_id {
                actions::chat::admit_chat(runtime, index, player, text, decoded_at);
            } else {
                disconnect_invalid_sequence(runtime, index);
            }
        }
        _ => {
            disconnect_invalid_sequence(runtime, index);
        }
    }
}

fn disconnect_invalid_sequence(runtime: &mut ServerRuntime, index: usize) {
    mark_disconnected(
        index,
        &mut runtime.connections.clients[index],
        &mut runtime.connections.disconnected,
        Some(CloseReason::InvalidSequence),
    );
}
