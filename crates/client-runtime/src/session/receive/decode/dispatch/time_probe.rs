use super::*;

pub(super) fn handle_time_probe_reply(
    session: &mut ClientSession,
    message: &ServerMessage,
    decoded_at_ms: u64,
    events: &mut Vec<SessionEvent>,
) -> DispatchResult {
    let ServerMessage::TimeProbeReply {
        seq,
        echoed_client_send_monotonic_ms,
        server_receive_monotonic_ms,
        server_send_monotonic_ms,
        session_nonce,
    } = message
    else {
        return DispatchResult::Stop;
    };
    if let Err(reason) = session.validate_probe_nonce(*session_nonce) {
        session.disconnect(reason, events);
        return DispatchResult::Stop;
    }
    session.clock.accept_probe_reply(
        *seq,
        *echoed_client_send_monotonic_ms,
        *server_receive_monotonic_ms,
        *server_send_monotonic_ms,
        decoded_at_ms,
    );
    DispatchResult::Emit
}
