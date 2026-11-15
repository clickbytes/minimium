pub(super) mod chat;
pub(super) mod combat;
pub(super) mod movement;

use crate::{
    connection::{mark_disconnected, queue_message, CloseReason},
    runtime::ServerRuntime,
};
use minimium_protocol::{ServerErrorCode, ServerMessage};

pub(super) fn reject(runtime: &mut ServerRuntime, index: usize, code: ServerErrorCode) {
    runtime.metrics.rejected_inputs = runtime.metrics.rejected_inputs.saturating_add(1);
    if !queue_message(
        &mut runtime.connections.clients[index],
        &mut runtime.metrics,
        ServerMessage::Error(code),
    ) {
        runtime.metrics.dropped_messages = runtime.metrics.dropped_messages.saturating_add(1);
    }
}

pub(super) fn reject_invalid_action(runtime: &mut ServerRuntime, index: usize) {
    mark_disconnected(
        index,
        &mut runtime.connections.clients[index],
        &mut runtime.connections.disconnected,
        Some(CloseReason::InvalidSequence),
    );
}
