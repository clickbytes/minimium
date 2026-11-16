use super::*;

pub(super) fn dispatch(
    session: &mut ClientSession,
    message: &ServerMessage,
    events: &mut Vec<SessionEvent>,
) -> DispatchResult {
    match message {
        ServerMessage::ImpactSync { context, complete } => {
            if session.world_context != Some(*context) {
                return DispatchResult::Skip;
            }
            if !complete {
                session.impacts.begin(*context);
            } else if session.impacts.context == Some(*context) {
                session.impacts.ready = true;
            }
            DispatchResult::Skip
        }
        ServerMessage::ImpactState {
            context,
            body,
            server_time_ms,
            ..
        } => {
            if session.world_context != Some(*context) {
                return DispatchResult::Skip;
            }
            let mismatched = session
                .resolved_world_content_for_context(*context)
                .is_some_and(|content| {
                    content.manifest().generation().seed().get() != body.world_seed
                        || content.manifest().planet().is_none_or(|p| {
                            (p.radius_mm as f64 / 1000.0 - body.planet_radius_m).abs() > 0.001
                        })
                });
            if mismatched
                || session
                    .impacts
                    .apply(*context, *body, *server_time_ms)
                    .is_err()
            {
                session.disconnect(DisconnectReason::UnexpectedMessage, events);
                return DispatchResult::Stop;
            }
            DispatchResult::Emit
        }
        _ => unreachable!("impact dispatch only"),
    }
}
