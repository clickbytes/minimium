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
            input: ClientWorldInput::SetMovementPolicy { speed, accel },
        } => {
            let Some(player) = player_id else {
                disconnect_invalid_sequence(runtime, index);
                return RoutingOutcome::Handled;
            };
            if !runtime
                .game
                .worlds
                .world_input_context_is_current(player, context)
            {
                reject_input(runtime);
                return RoutingOutcome::Handled;
            }
            if !super::identity::has_permission(
                runtime,
                index,
                minimium_core::identity::ServerPermission::DeveloperMovementPolicy,
                "movement_policy",
            ) {
                return RoutingOutcome::Handled;
            }
            let Some(previous) = runtime.game.player_movement_policies.get(&player).copied() else {
                reject_input(runtime);
                return RoutingOutcome::Handled;
            };
            let Some(revision) = previous.revision.checked_add(1) else {
                reject_input(runtime);
                return RoutingOutcome::Handled;
            };
            let policy = minimium_simulation::MovementPolicy {
                speed,
                accel,
                revision,
                ..previous
            };
            if !policy.valid() {
                reject_input(runtime);
                return RoutingOutcome::Handled;
            }
            runtime.game.player_movement_policies.insert(player, policy);
            if !queue_message(
                &mut runtime.connections.clients[index],
                &mut runtime.metrics,
                ServerMessage::MovementPolicy(policy),
            ) {
                disconnect_backpressure(runtime, index);
            }
            RoutingOutcome::Handled
        }
        ClientMessage::WorldInput {
            context,
            input: ClientWorldInput::SetFlightEnabled(enabled),
        } => {
            let Some(player) = player_id else {
                disconnect_invalid_sequence(runtime, index);
                return RoutingOutcome::Handled;
            };
            if !runtime
                .game
                .worlds
                .world_input_context_is_current(player, context)
            {
                reject_input(runtime);
                return RoutingOutcome::Handled;
            }
            if !super::identity::has_permission(
                runtime,
                index,
                minimium_core::identity::ServerPermission::DeveloperFly,
                "fly",
            ) {
                return RoutingOutcome::Handled;
            }
            let Some(previous) = runtime.game.player_movement_policies.get(&player).copied() else {
                reject_input(runtime);
                return RoutingOutcome::Handled;
            };
            let Some(revision) = previous.revision.checked_add(1) else {
                reject_input(runtime);
                return RoutingOutcome::Handled;
            };
            let policy = minimium_simulation::MovementPolicy {
                can_fly: enabled,
                revision,
                ..previous
            };
            runtime.game.player_movement_policies.insert(player, policy);
            if !queue_message(
                &mut runtime.connections.clients[index],
                &mut runtime.metrics,
                ServerMessage::MovementPolicy(policy),
            ) {
                disconnect_backpressure(runtime, index);
            }
            RoutingOutcome::Handled
        }
        ClientMessage::TransitionReady {
            transition_id,
            target_context,
        } => {
            let accepted = player_id.is_some_and(|player| {
                runtime
                    .game
                    .worlds
                    .mark_transfer_ready(player, transition_id, target_context)
            });
            if !accepted {
                reject_input(runtime);
            }
            RoutingOutcome::Handled
        }
        ClientMessage::CancelTransition(transition_id) => {
            if let Some(player) = player_id {
                if runtime.game.worlds.cancel_transfer(player, transition_id) {
                    if let Some(context) = runtime.game.worlds.context_for_player(player) {
                        let _ = queue_message(
                            &mut runtime.connections.clients[index],
                            &mut runtime.metrics,
                            ServerMessage::TransitionCancelled {
                                transition_id,
                                context,
                            },
                        );
                    }
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
