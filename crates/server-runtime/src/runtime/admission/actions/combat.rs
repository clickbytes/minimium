use crate::runtime::{
    action_state::ActorAction, action_state::ActorActionAdmission, ServerRuntime,
};
use minimium_core::ids::PlayerId;
use minimium_protocol::ServerErrorCode;
use std::time::Instant;

pub(in crate::runtime::admission) fn admit_fire(
    runtime: &mut ServerRuntime,
    index: usize,
    player: PlayerId,
    direction: [f32; 3],
    decoded_at: Instant,
) {
    let now = runtime.now;
    let reserved_transport_inputs = super::super::queue::reserved_transport_inputs(runtime);
    match runtime.game.admit_actor_action(
        player,
        ActorAction::Fire(direction),
        decoded_at,
        now,
        reserved_transport_inputs,
    ) {
        ActorActionAdmission::Queued => {}
        ActorActionAdmission::InvalidIntent => super::reject_invalid_action(runtime, index),
        ActorActionAdmission::ActorUnavailable => {
            runtime.metrics.rejected_inputs = runtime.metrics.rejected_inputs.saturating_add(1);
        }
        ActorActionAdmission::RateLimited => {
            super::reject(runtime, index, ServerErrorCode::RateLimit)
        }
        ActorActionAdmission::Busy => super::reject(runtime, index, ServerErrorCode::Busy),
    }
}
