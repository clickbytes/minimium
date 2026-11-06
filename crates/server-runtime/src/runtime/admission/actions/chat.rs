use crate::{
    connection::queue_message, runtime::ServerRuntime, world_inputs::WorldInput, CHAT_COOLDOWN,
    GLOBAL_CHAT_COOLDOWN,
};
use minimium_core::ids::PlayerId;
use minimium_protocol::ServerMessage;
use std::time::Instant;

use super::super::queue;

pub(in crate::runtime::admission) fn admit_chat(
    runtime: &mut ServerRuntime,
    index: usize,
    player: PlayerId,
    text: String,
    decoded_at: Instant,
) {
    let now = Instant::now();
    let rate_limited = runtime.connections.clients[index]
        .last_chat_at
        .is_some_and(|last_chat| now.saturating_duration_since(last_chat) < CHAT_COOLDOWN)
        || runtime.game.last_global_chat_at.is_some_and(|last_chat| {
            now.saturating_duration_since(last_chat) < GLOBAL_CHAT_COOLDOWN
        });
    if rate_limited {
        runtime.metrics.rejected_inputs = runtime.metrics.rejected_inputs.saturating_add(1);
        if !queue_message(
            &mut runtime.connections.clients[index],
            &mut runtime.metrics,
            ServerMessage::Error(minimium_protocol::ServerErrorCode::RateLimit),
        ) {
            runtime.metrics.dropped_messages = runtime.metrics.dropped_messages.saturating_add(1);
        }
    } else if queue::try_queue(runtime, WorldInput::Chat(player, text), decoded_at) {
        runtime.connections.clients[index].last_chat_at = Some(now);
        runtime.game.last_global_chat_at = Some(now);
    } else {
        runtime.metrics.rejected_inputs = runtime.metrics.rejected_inputs.saturating_add(1);
        if !queue_message(
            &mut runtime.connections.clients[index],
            &mut runtime.metrics,
            ServerMessage::Error(minimium_protocol::ServerErrorCode::Busy),
        ) {
            runtime.metrics.dropped_messages = runtime.metrics.dropped_messages.saturating_add(1);
        }
    }
}
