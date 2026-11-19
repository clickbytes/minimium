use super::{actions, identity, routing::RoutingOutcome, technical, world_control};
use crate::{
    connection::{mark_disconnected, CloseReason},
    runtime::ServerRuntime,
};
use minimium_core::ids::PlayerId;
use minimium_protocol::{ClientMessage, ClientWorldInput};
use std::time::Instant;

pub(super) fn dispatch_message(
    runtime: &mut ServerRuntime,
    index: usize,
    message: ClientMessage,
    decoded_at: Instant,
) {
    // A poll may decode several frames before Quit or another close request is
    // dispatched. Do not admit later frames from the same batch while draining.
    if runtime.connections.clients[index]
        .close_deadline()
        .is_some()
    {
        return;
    }
    let player_id = runtime.connections.clients[index].player_id();
    if player_id.is_some()
        && !identity::has_permission(
            runtime,
            index,
            minimium_core::identity::ServerPermission::Gameplay,
            "gameplay",
        )
    {
        return;
    }
    // Routing preserves wrapper conversion and pre-dispatch effects; only continued messages reach admission below.
    let message = match identity::negotiate_content(runtime, index, message, player_id, decoded_at)
    {
        RoutingOutcome::Handled => return,
        RoutingOutcome::ContinueWith(message) => message,
    };
    if let ClientMessage::WorldInput {
        sequence,
        control_revision,
        context,
        input,
    } = &message
    {
        // TCP preserves order. Consume the id even if this action is refused or paused.
        let client = &mut runtime.connections.clients[index];
        if *sequence <= client.world_sequence {
            return;
        }
        client.world_sequence = *sequence;
        if !runtime.environment_clock.accepts_input(*control_revision)
            && player_id.is_some()
            && !matches!(
                input,
                ClientWorldInput::Chat(_)
                    | ClientWorldInput::SetName(_)
                    | ClientWorldInput::Excavation(
                        minimium_protocol::ExcavationInput::Resync { .. }
                    )
            )
        {
            if let (
                Some(player),
                ClientWorldInput::Excavation(minimium_protocol::ExcavationInput::Use {
                    action_id,
                    ..
                }),
            ) = (player_id, input)
            {
                runtime.excavation_result(
                    player,
                    *context,
                    minimium_world::items::ActionResult::Rejected {
                        action_id: *action_id,
                        reason: minimium_world::items::ActionRejection::Paused,
                    },
                );
            } else {
                crate::connection::queue_message(
                    &mut runtime.connections.clients[index],
                    &mut runtime.metrics,
                    minimium_protocol::ServerMessage::Error(
                        minimium_protocol::ServerErrorCode::Paused,
                    ),
                );
            }
            return;
        }
    }
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
        ClientMessage::Authenticate { .. } => {
            disconnect_invalid_sequence(runtime, index);
        }
        ClientMessage::WorldInput { context, input, .. } => match (player_id, input) {
            (Some(player), ClientWorldInput::Excavation(input)) => {
                if matches!(input, minimium_protocol::ExcavationInput::Resync { .. }) {
                    runtime.apply_excavation(player, context, input);
                    return;
                }
                if matches!(input, minimium_protocol::ExcavationInput::GrantTools)
                    && !identity::has_permission(
                        runtime,
                        index,
                        minimium_core::identity::ServerPermission::DeveloperTeleport,
                        "developer inventory",
                    )
                {
                    return;
                }
                let action_id = match &input {
                    minimium_protocol::ExcavationInput::Use { action_id, .. } => Some(*action_id),
                    _ => None,
                };
                if !super::queue::try_queue(
                    runtime,
                    crate::world_inputs::WorldInput::Excavation(player, context, input),
                    decoded_at,
                ) {
                    if let Some(action_id) = action_id {
                        runtime.excavation_result(
                            player,
                            context,
                            minimium_world::items::ActionResult::Rejected {
                                action_id,
                                reason: minimium_world::items::ActionRejection::Busy,
                            },
                        );
                    }
                }
            }
            (Some(player), ClientWorldInput::SetName(name)) => {
                actions::movement::admit_name(runtime, index, player, name, decoded_at)
            }
            (Some(player), ClientWorldInput::Motion(input)) => {
                actions::movement::admit_motion(runtime, index, player, input, decoded_at)
            }
            (Some(player), ClientWorldInput::Fire { direction }) => {
                actions::combat::admit_fire(runtime, index, player, direction, decoded_at)
            }
            (Some(player), ClientWorldInput::Chat(text)) => {
                actions::chat::admit_chat(runtime, index, player, text, decoded_at)
            }
            (
                Some(_),
                ClientWorldInput::InteractEntity { .. }
                | ClientWorldInput::SetAltitude { .. }
                | ClientWorldInput::MapTeleport { .. }
                | ClientWorldInput::SetMovementPolicy { .. }
                | ClientWorldInput::SetFlightEnabled(_),
            ) => disconnect_invalid_sequence(runtime, index),
            (None, _) => disconnect_invalid_sequence(runtime, index),
        },
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
