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
    context: minimium_protocol::WorldContext,
    sequence: u64,
    name: String,
    decoded_at: Instant,
) -> bool {
    if queue::try_queue(
        runtime,
        WorldInput::Rename(player, context, sequence, name),
        decoded_at,
    ) {
        if !queue_message(
            &mut runtime.connections.clients[index],
            &mut runtime.metrics,
            ServerMessage::Accepted,
        ) {
            runtime.metrics.dropped_messages = runtime.metrics.dropped_messages.saturating_add(1);
        }
        true
    } else {
        runtime.metrics.rejected_inputs = runtime.metrics.rejected_inputs.saturating_add(1);
        let _ = queue_message(
            &mut runtime.connections.clients[index],
            &mut runtime.metrics,
            ServerMessage::Error(ServerErrorCode::Busy),
        );
        false
    }
}

pub(in crate::runtime::admission) fn admit_motion(
    runtime: &mut ServerRuntime,
    index: usize,
    player: PlayerId,
    context: minimium_protocol::WorldContext,
    sequence: u64,
    mut sample: MovementSample,
    decoded_at: Instant,
) -> Result<(), minimium_protocol::MotionRejection> {
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
    let current_tick = runtime.environment_clock.boundary().tick();
    if sample.input_seq == 0
        || sample.input_seq
            <= runtime
                .game
                .last_input_seq
                .get(&player)
                .copied()
                .unwrap_or(0)
    {
        runtime.metrics.rejected_inputs = runtime.metrics.rejected_inputs.saturating_add(1);
        return Err(minimium_protocol::MotionRejection::InvalidSequence);
    }
    sample.target_tick = current_tick + 1;
    runtime.game.input_last_received.insert(player, decoded_at);
    if !queue::try_queue(
        runtime,
        WorldInput::Move(player, context, sequence, sample),
        decoded_at,
    ) {
        runtime.game.last_input_seq.insert(player, sample.input_seq);
        runtime.resolve_movement_input(player, sample.input_seq);
        Err(minimium_protocol::MotionRejection::Busy)
    } else {
        runtime.game.last_input_seq.insert(player, sample.input_seq);
        runtime
            .game
            .pending_input_seqs
            .entry(player)
            .or_default()
            .insert(sample.input_seq);
        Ok(())
    }
}

pub(in crate::runtime::admission) fn admit_jump(
    runtime: &mut ServerRuntime,
    index: usize,
    player: PlayerId,
    context: minimium_protocol::WorldContext,
    sequence: u64,
    input_seq: u64,
    decoded_at: Instant,
) {
    use crate::runtime::movement_receipts::Ticket;
    use minimium_protocol::{JumpOutcome, MotionRejection};
    let ticket = Ticket {
        context,
        request_id: sequence,
        input_seq,
        scheduled: false,
    };
    let result = if runtime
        .game
        .jumps
        .get(&player)
        .is_some_and(|pending| pending.len() >= 32)
    {
        Err(MotionRejection::Busy)
    } else {
        let sample = MovementSample {
            input_seq,
            target_tick: 0,
            intent: minimium_simulation::MovementIntent {
                jump: true,
                ..Default::default()
            },
        };
        admit_motion(
            runtime, index, player, context, sequence, sample, decoded_at,
        )
    };
    let outcome = match result {
        Ok(()) => {
            runtime.game.jumps.entry(player).or_default().push(ticket);
            JumpOutcome::Accepted
        }
        Err(reason) => JumpOutcome::Rejected(reason),
    };
    runtime.send_jump(index, ticket, outcome);
}
