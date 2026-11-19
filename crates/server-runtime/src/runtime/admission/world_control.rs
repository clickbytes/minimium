use super::{queue, routing::RoutingOutcome};
use crate::{
    connection::{mark_disconnected, queue_message, CloseReason},
    runtime::ServerRuntime,
    world_inputs::WorldInput,
};
use minimium_core::ids::PlayerId;
use minimium_protocol::{ClientMessage, ClientWorldInput, ServerMessage};
use std::time::Instant;

pub(super) fn route_message(
    runtime: &mut ServerRuntime,
    index: usize,
    message: ClientMessage,
    decoded_at: Instant,
    player_id: Option<PlayerId>,
) -> RoutingOutcome {
    match message {
        ClientMessage::WorldInput {
            context,
            input: ClientWorldInput::InteractEntity { entity },
            ..
        } => {
            let Some(player) = player_id else {
                disconnect_invalid_sequence(runtime, index);
                return RoutingOutcome::Handled;
            };
            if runtime
                .game
                .worlds
                .world_input_context_is_current(player, context)
                && queue::try_queue(
                    runtime,
                    WorldInput::InteractEntity(player, context, entity),
                    decoded_at,
                )
            {
                // The game module validates entity type and interaction range on the owner lane.
            } else {
                reject_input(runtime);
            }
            RoutingOutcome::Handled
        }
        ClientMessage::WorldInput {
            context,
            input: ClientWorldInput::SetAltitude { km },
            ..
        } => {
            if !super::identity::has_permission(
                runtime,
                index,
                minimium_core::identity::ServerPermission::DeveloperTeleport,
                "set_altitude",
            ) {
                return RoutingOutcome::Handled;
            }
            let Some(player) = player_id else {
                disconnect_invalid_sequence(runtime, index);
                return RoutingOutcome::Handled;
            };
            if !runtime
                .game
                .worlds
                .world_input_context_is_current(player, context)
                || !queue::try_queue(
                    runtime,
                    WorldInput::SetAltitude(player, context, km),
                    decoded_at,
                )
            {
                reject_input(runtime);
            }
            RoutingOutcome::Handled
        }
        ClientMessage::WorldInput {
            context,
            input: ClientWorldInput::MapTeleport { direction },
            ..
        } => {
            if !super::identity::has_permission(
                runtime,
                index,
                minimium_core::identity::ServerPermission::DeveloperTeleport,
                "map_teleport",
            ) {
                reject_map_teleport(runtime, index);
                return RoutingOutcome::Handled;
            }
            let Some(player) = player_id else {
                disconnect_invalid_sequence(runtime, index);
                return RoutingOutcome::Handled;
            };
            if !runtime
                .game
                .worlds
                .world_input_context_is_current(player, context)
                || !queue::try_queue(
                    runtime,
                    WorldInput::MapTeleport(player, context, direction),
                    decoded_at,
                )
            {
                reject_input(runtime);
                reject_map_teleport(runtime, index);
            }
            RoutingOutcome::Handled
        }
        ClientMessage::WorldInput {
            context,
            sequence,
            input,
            ..
        } if input.policy_change().is_some() => {
            let (patch, expected) = input.policy_change().expect("matched policy input");
            super::movement_policy::apply(
                runtime, index, player_id, context, sequence, patch, expected,
            );
            RoutingOutcome::Handled
        }
        ClientMessage::TransitionReady {
            transition_id,
            target_context,
        } => {
            let accepted = player_id.is_some_and(|player| {
                runtime.game.worlds.mark_transfer_ready(
                    player,
                    transition_id,
                    target_context,
                    runtime.now,
                )
            });
            if !accepted {
                reject_input(runtime);
            }
            RoutingOutcome::Handled
        }
        ClientMessage::CancelTransition(transition_id) => {
            if let Some(player) = player_id {
                if let Some(cancelled) = runtime.game.worlds.cancel_transfer(player, transition_id)
                {
                    runtime.finish_transfer_cancellation(cancelled);
                }
            }
            RoutingOutcome::Handled
        }
        other => RoutingOutcome::ContinueWith(other),
    }
}

fn reject_input(runtime: &mut ServerRuntime) {
    runtime.metrics.rejected_inputs = runtime.metrics.rejected_inputs.saturating_add(1);
}

fn reject_map_teleport(runtime: &mut ServerRuntime, index: usize) {
    if !queue_message(
        &mut runtime.connections.clients[index],
        &mut runtime.metrics,
        ServerMessage::MapTeleportResult { position_mm: None },
    ) {
        disconnect_backpressure(runtime, index);
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

fn disconnect_backpressure(runtime: &mut ServerRuntime, index: usize) {
    mark_disconnected(
        index,
        &mut runtime.connections.clients[index],
        &mut runtime.connections.disconnected,
        Some(CloseReason::Backpressure),
    );
}
