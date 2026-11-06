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
            input: ClientWorldInput::UsePortal,
        } => {
            let Some(player) = player_id else {
                disconnect_invalid_sequence(runtime, index);
                return RoutingOutcome::Handled;
            };
            if runtime
                .game
                .worlds
                .world_input_context_is_current(player, context)
                && queue::try_queue(runtime, WorldInput::UsePortal(player, context), decoded_at)
            {
                // The queued portal input will be applied in the normal world-input phase.
            } else {
                reject_input(runtime);
            }
            RoutingOutcome::Handled
        }
        ClientMessage::WorldInput {
            context,
            input: ClientWorldInput::SetAltitude { km },
        } => {
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

fn disconnect_invalid_sequence(runtime: &mut ServerRuntime, index: usize) {
    mark_disconnected(
        index,
        &mut runtime.connections.clients[index],
        &mut runtime.connections.disconnected,
        Some(CloseReason::InvalidSequence),
    );
}
