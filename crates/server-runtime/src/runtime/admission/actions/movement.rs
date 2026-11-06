use crate::{connection::queue_message, runtime::ServerRuntime, world_inputs::WorldInput};
use minimium_core::ids::PlayerId;
use minimium_protocol::{ServerErrorCode, ServerMessage};
use minimium_simulation::CharacterInput;
use std::time::Instant;

use super::super::queue;

pub(in crate::runtime::admission) fn admit_name(
    runtime: &mut ServerRuntime,
    index: usize,
    player: PlayerId,
    name: String,
    decoded_at: Instant,
) {
    if queue::try_queue(runtime, WorldInput::Rename(player, name), decoded_at) {
        if !queue_message(
            &mut runtime.connections.clients[index],
            &mut runtime.metrics,
            ServerMessage::Accepted,
        ) {
            runtime.metrics.dropped_messages = runtime.metrics.dropped_messages.saturating_add(1);
        }
    } else {
        runtime.metrics.rejected_inputs = runtime.metrics.rejected_inputs.saturating_add(1);
        let _ = queue_message(
            &mut runtime.connections.clients[index],
            &mut runtime.metrics,
            ServerMessage::Error(ServerErrorCode::Busy),
        );
    }
}

pub(in crate::runtime::admission) fn admit_motion(
    runtime: &mut ServerRuntime,
    index: usize,
    player: PlayerId,
    input: CharacterInput,
    decoded_at: Instant,
) {
    if !queue::try_queue(runtime, WorldInput::Move(player, input), decoded_at) {
        let _ = queue_message(
            &mut runtime.connections.clients[index],
            &mut runtime.metrics,
            ServerMessage::Error(ServerErrorCode::Busy),
        );
    }
}
