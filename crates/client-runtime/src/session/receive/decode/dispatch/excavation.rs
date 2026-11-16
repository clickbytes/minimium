use super::*;
use minimium_protocol::{ClientWorldInput, ExcavationInput, ExcavationMessage};

pub(super) fn dispatch(
    session: &mut ClientSession,
    message: &ServerMessage,
    _events: &mut Vec<SessionEvent>,
) -> DispatchResult {
    let ServerMessage::Excavation { context, message } = message else {
        unreachable!("excavation dispatch only")
    };
    if session.state != SessionState::Connected
        || session.world_context != Some(*context)
        || session.pending_transition.is_some()
    {
        return DispatchResult::Skip;
    }
    let result = session.excavation.apply(*context, message.clone());
    if let Err(error) = result {
        let is_replication = matches!(
            message,
            ExcavationMessage::BatchBegin { .. }
                | ExcavationMessage::ChunkBegin { .. }
                | ExcavationMessage::ChunkPart { .. }
                | ExcavationMessage::ChunkEnd { .. }
                | ExcavationMessage::BatchEnd { .. }
                | ExcavationMessage::FluidBegin { .. }
                | ExcavationMessage::FluidPart { .. }
                | ExcavationMessage::FluidEnd { .. }
                | ExcavationMessage::Crater { .. }
        );
        if is_replication && session.excavation.reject_batch_message() {
            let input = ClientWorldInput::Excavation(ExcavationInput::Resync {
                coordinates: [0, 0, 0],
            });
            let _ = session.queue_world_input(input, session.now);
            return DispatchResult::Skip;
        }
        if let Some(logger) = &session.diagnostic_logger {
            crate::diagnostics::emit(
                Some(logger),
                minimium_diagnostics::Level::Warn,
                "client.excavation_rejected",
                [crate::diagnostics::field("reason", error)],
            );
        }
        return DispatchResult::Skip;
    }
    if let Some(result) = session.excavation.take_result() {
        session.receive_tool_result(*context, result);
    }
    if matches!(
        message,
        ExcavationMessage::BatchEnd { .. }
            | ExcavationMessage::Inventory { .. }
            | ExcavationMessage::ActionResult { .. }
    ) {
        DispatchResult::Emit
    } else {
        DispatchResult::Skip
    }
}
