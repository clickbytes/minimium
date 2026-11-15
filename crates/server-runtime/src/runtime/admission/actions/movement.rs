use crate::{connection::queue_message, runtime::ServerRuntime, world_inputs::WorldInput};
use minimium_core::ids::PlayerId;
use minimium_protocol::MovementSample;
use minimium_protocol::{ServerErrorCode, ServerMessage};
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
    mut sample: MovementSample,
    decoded_at: Instant,
) {
    use minimium_core::identity::ServerPermission;
    if sample.intent.vertical != 0.0 {
        if !super::super::identity::has_permission(
            runtime,
            index,
            ServerPermission::DeveloperFly,
            "fly",
        ) {
            sample.intent.vertical = 0.0;
        }
    }
    let current_tick = runtime.scheduler.simulation_tick;
    if sample.input_seq == 0
        || sample.input_seq
            <= runtime
                .game
                .last_input_seq
                .get(&player)
                .copied()
                .unwrap_or(0)
        || sample.target_tick <= current_tick
        || sample.target_tick > current_tick.saturating_add(8)
    {
        runtime.metrics.rejected_inputs = runtime.metrics.rejected_inputs.saturating_add(1);
        if sample.input_seq
            > runtime
                .game
                .last_input_seq
                .get(&player)
                .copied()
                .unwrap_or(0)
        {
            runtime.game.last_input_seq.insert(player, sample.input_seq);
            runtime.resolve_movement_input(player, sample.input_seq);
        }
        return;
    }
    if !queue::try_queue(runtime, WorldInput::Move(player, sample), decoded_at) {
        runtime.game.last_input_seq.insert(player, sample.input_seq);
        runtime.resolve_movement_input(player, sample.input_seq);
        let _ = queue_message(
            &mut runtime.connections.clients[index],
            &mut runtime.metrics,
            ServerMessage::Error(ServerErrorCode::Busy),
        );
    } else {
        runtime.game.last_input_seq.insert(player, sample.input_seq);
        runtime
            .game
            .pending_input_seqs
            .entry(player)
            .or_default()
            .insert(sample.input_seq);
    }
}
