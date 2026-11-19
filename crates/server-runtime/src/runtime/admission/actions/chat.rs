use crate::runtime::{
    action_state::ActorAction, action_state::ActorActionAdmission, ServerRuntime,
};
use minimium_core::ids::PlayerId;
use minimium_protocol::ServerErrorCode;
use std::time::Instant;

pub(in crate::runtime::admission) fn admit_chat(
    runtime: &mut ServerRuntime,
    index: usize,
    player: PlayerId,
    context: minimium_protocol::WorldContext,
    sequence: u64,
    text: String,
    decoded_at: Instant,
) -> ActorActionAdmission {
    let now = runtime.now;
    let reserved_transport_inputs = super::super::queue::reserved_transport_inputs(runtime);
    let outcome = runtime.game.admit_actor_action(
        player,
        context,
        sequence,
        ActorAction::Chat(text),
        decoded_at,
        now,
        runtime.environment_clock.boundary().simulation_ms(),
        reserved_transport_inputs,
    );
    match outcome {
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
    outcome
}
