use crate::{runtime::ServerRuntime, world_inputs::WorldInput, MAX_PENDING_WORLD_INPUTS};
use std::time::Instant;

pub(super) fn try_queue(
    runtime: &mut ServerRuntime,
    input: WorldInput,
    decoded_at: Instant,
) -> bool {
    if matches!(&input, WorldInput::Fire(..) | WorldInput::Chat(..)) {
        return false;
    }
    if !has_capacity(runtime) {
        return false;
    }
    runtime
        .game
        .pending_world_inputs
        .push((input, Some(decoded_at)));
    true
}

pub(super) fn has_capacity(runtime: &ServerRuntime) -> bool {
    runtime
        .game
        .pending_world_inputs
        .len()
        .saturating_add(reserved_transport_inputs(runtime))
        < MAX_PENDING_WORLD_INPUTS
}

pub(super) fn reserved_transport_inputs(runtime: &ServerRuntime) -> usize {
    runtime.connections.clients.len()
}
