use crate::{
    connection::{mark_disconnected, queue_message, CloseReason},
    runtime::ServerRuntime,
};
use minimium_core::{identity::ServerPermission, ids::PlayerId};
use minimium_protocol::{
    MovementPatch, PolicyRejection as Rejection, PolicyReply, ServerMessage, WorldContext,
};
use minimium_simulation::MovementPolicy;

pub(super) fn apply(
    runtime: &mut ServerRuntime,
    index: usize,
    player: Option<PlayerId>,
    context: WorldContext,
    request_id: u64,
    patch: MovementPatch,
    expected: Option<u64>,
) {
    let result = change(runtime, index, player, context, patch, expected);
    if result.is_err() && result != Err(Rejection::Forbidden) {
        runtime.metrics.rejected_inputs = runtime.metrics.rejected_inputs.saturating_add(1);
    }
    reply(runtime, index, context, request_id, result);
}

fn change(
    runtime: &mut ServerRuntime,
    index: usize,
    player: Option<PlayerId>,
    context: WorldContext,
    patch: MovementPatch,
    expected: Option<u64>,
) -> Result<MovementPolicy, Rejection> {
    let player = player
        .filter(|p| {
            runtime
                .game
                .worlds
                .world_input_context_is_current(*p, context)
        })
        .ok_or(Rejection::StaleContext)?;
    let permission = if matches!(patch, MovementPatch::FlightEnabled(_)) {
        ServerPermission::DeveloperFly
    } else {
        ServerPermission::DeveloperMovementPolicy
    };
    if !super::identity::has_permission(runtime, index, permission, "movement_policy") {
        return Err(Rejection::Forbidden);
    }
    let policy = runtime
        .game
        .player_movement_policies
        .get_mut(&player)
        .ok_or(Rejection::Busy)?;
    if expected.is_some_and(|revision| revision != policy.revision) {
        return Err(Rejection::RevisionConflict);
    }
    let mut next = patch.apply(*policy).ok_or(Rejection::InvalidValue)?;
    next.revision = policy.revision.checked_add(1).ok_or(Rejection::Busy)?;
    *policy = next;
    Ok(next)
}

pub(super) fn reply(
    runtime: &mut ServerRuntime,
    index: usize,
    context: WorldContext,
    request_id: u64,
    result: Result<MovementPolicy, Rejection>,
) {
    let client = &mut runtime.connections.clients[index];
    if !queue_message(
        client,
        &mut runtime.metrics,
        ServerMessage::PolicyReply(PolicyReply {
            context,
            request_id,
            result,
        }),
    ) {
        mark_disconnected(
            index,
            client,
            &mut runtime.connections.disconnected,
            Some(CloseReason::Backpressure),
        );
    }
}
