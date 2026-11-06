use crate::{runtime::ServerRuntime, world_inputs::WorldInput, MAX_PENDING_WORLD_INPUTS};
use std::time::Instant;

pub(super) fn try_queue(
    runtime: &mut ServerRuntime,
    input: WorldInput,
    decoded_at: Instant,
) -> bool {
    if runtime.game.pending_world_inputs.len() + runtime.connections.clients.len()
        >= MAX_PENDING_WORLD_INPUTS
    {
        return false;
    }
    runtime
        .game
        .pending_world_inputs
        .push((input, Some(decoded_at)));
    true
}
