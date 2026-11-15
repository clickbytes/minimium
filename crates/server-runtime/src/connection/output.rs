use super::*;

pub(crate) fn queue_close_response(
    client: &mut Client,
    metrics: &mut RuntimeMetrics,
    message: ServerMessage,
    now: Instant,
) -> bool {
    let reason = if matches!(message, ServerMessage::Bye) {
        CloseReason::Quit
    } else {
        CloseReason::Rejected
    };
    if !queue_message_with_deadline(client, metrics, message, Some(now + CLIENT_CLOSE_TIMEOUT)) {
        metrics.dropped_messages = metrics.dropped_messages.saturating_add(1);
        client.disconnect_reason = Some(CloseReason::Backpressure);
        return false;
    }
    client.begin_close(now, reason);
    true
}
pub(crate) fn mark_disconnected(
    index: usize,
    client: &mut Client,
    disconnected: &mut Vec<usize>,
    reason: Option<CloseReason>,
) {
    client.transport.close(client.connection_id());
    if let Some(reason) = reason {
        client.disconnect_reason = Some(reason);
    }
    disconnected.push(index);
}

pub(crate) fn queue_message(
    client: &mut Client,
    metrics: &mut RuntimeMetrics,
    message: ServerMessage,
) -> bool {
    queue_message_with_deadline(client, metrics, message, None)
}

pub(crate) fn queue_message_with_deadline(
    client: &mut Client,
    metrics: &mut RuntimeMetrics,
    message: ServerMessage,
    close_after_flush: Option<Instant>,
) -> bool {
    queue_message_with_budget(
        client,
        metrics,
        message,
        0,
        minimium_protocol::MAX_SERVER_FRAME_BYTES,
        minimium_protocol::MAX_SERVER_FRAME_BYTES,
        close_after_flush,
    )
    .is_ok()
}

pub(crate) fn queue_message_with_reserve(
    client: &mut Client,
    metrics: &mut RuntimeMetrics,
    message: ServerMessage,
    reserved_bytes: usize,
) -> bool {
    queue_message_with_limit(
        client,
        metrics,
        message,
        reserved_bytes,
        minimium_protocol::MAX_SERVER_FRAME_BYTES,
        None,
    )
    .is_ok()
}

pub(crate) fn queue_message_with_limit(
    client: &mut Client,
    metrics: &mut RuntimeMetrics,
    message: ServerMessage,
    reserved_bytes: usize,
    max_frame_bytes: usize,
    close_after_flush: Option<Instant>,
) -> Result<usize, minimium_transport::QueueError> {
    queue_message_with_budget(
        client,
        metrics,
        message,
        reserved_bytes,
        0,
        max_frame_bytes,
        close_after_flush,
    )
}

fn queue_message_with_budget(
    client: &mut Client,
    metrics: &mut RuntimeMetrics,
    message: ServerMessage,
    reserved_bytes: usize,
    minimum_headroom_bytes: usize,
    max_frame_bytes: usize,
    close_after_flush: Option<Instant>,
) -> Result<usize, minimium_transport::QueueError> {
    if client.close_deadline().is_some() {
        return Err(minimium_transport::QueueError::Closed);
    }
    match client.transport.queue_game(GameOutput {
        connection_id: client.connection_id,
        close_reason: close_after_flush.map(|_| {
            if matches!(&message, ServerMessage::Bye) {
                minimium_transport::GameDisconnectReason::Quit
            } else {
                minimium_transport::GameDisconnectReason::Rejected
            }
        }),
        message,
        close_after_flush,
        reserved_bytes,
        minimum_headroom_bytes,
        max_frame_bytes,
    }) {
        Ok(result) => {
            metrics.encoding.record(result.encoding_time);
            client.encoding_us = client
                .encoding_us
                .saturating_add(result.encoding_time.as_micros().min(u64::MAX as u128) as u64);
            metrics.frames_out = metrics.frames_out.saturating_add(1);
            Ok(result.encoded_bytes)
        }
        Err(error) => {
            if matches!(error, minimium_transport::QueueError::Encoding) {
                if let Some(logger) = &client.protocol_logger {
                    crate::diagnostics::emit(
                        logger,
                        minimium_diagnostics::Level::Error,
                        "protocol.message_encode_failed",
                        &[("reason", error.to_string())],
                    );
                }
            }
            Err(error)
        }
    }
}
