use crate::{connection::queue_message, runtime::ServerRuntime, world_inputs::WorldInput};
use minimium_core::ids::PlayerId;
use minimium_protocol::{ServerErrorCode, ServerMessage};
use std::time::Instant;

use super::super::queue;

pub(in crate::runtime::admission) fn admit_fire(
    runtime: &mut ServerRuntime,
    index: usize,
    player: PlayerId,
    direction: [f32; 3],
    decoded_at: Instant,
) {
    let now = Instant::now();
    if runtime.connections.clients[index]
        .last_fire_at
        .is_some_and(|last_fire| now.saturating_duration_since(last_fire) < runtime.shot_cooldown)
    {
        runtime.metrics.rejected_inputs = runtime.metrics.rejected_inputs.saturating_add(1);
        if !queue_message(
            &mut runtime.connections.clients[index],
            &mut runtime.metrics,
            ServerMessage::Error(ServerErrorCode::RateLimit),
        ) {
            runtime.metrics.dropped_messages = runtime.metrics.dropped_messages.saturating_add(1);
        }
    } else if queue::try_queue(runtime, WorldInput::Fire(player, direction), decoded_at) {
        runtime.connections.clients[index].last_fire_at = Some(now);
    } else {
        runtime.metrics.rejected_inputs = runtime.metrics.rejected_inputs.saturating_add(1);
        if !queue_message(
            &mut runtime.connections.clients[index],
            &mut runtime.metrics,
            ServerMessage::Error(ServerErrorCode::Busy),
        ) {
            runtime.metrics.dropped_messages = runtime.metrics.dropped_messages.saturating_add(1);
        }
    }
}
