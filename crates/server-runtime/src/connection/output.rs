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
    if !queue_message(client, metrics, message) {
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
    if client.close_deadline().is_some() {
        return false;
    }
    if client.output_frames.len() >= MAX_QUEUED_OUTPUT_FRAMES {
        return false;
    }
    prepare_output(client);
    if client.output.len() + MAX_SERVER_FRAME_BYTES > MAX_QUEUED_OUTPUT_BYTES {
        return false;
    }
    client.output.reserve_exact(MAX_SERVER_FRAME_BYTES);
    let frame_start = client.output.len();
    let encoding_started = Instant::now();
    let result = encode_server_into(&mut client.output, &message);
    let encoding_time = encoding_started.elapsed();
    metrics.encoding.record(encoding_time);
    client.encoding_us = client
        .encoding_us
        .saturating_add(encoding_time.as_micros().min(u64::MAX as u128) as u64);
    match result {
        Ok(()) if client.output.len() <= MAX_QUEUED_OUTPUT_BYTES => {
            client
                .output_frames
                .push_back((client.output.len() - frame_start, Instant::now()));
            client.frames_out += 1;
            metrics.frames_out += 1;
            true
        }
        Ok(()) => {
            client.output.truncate(frame_start);
            false
        }
        Err(error) => {
            client.output.truncate(frame_start);
            if let Some(logger) = &client.protocol_logger {
                crate::diagnostics::emit(
                    logger,
                    minimium_diagnostics::Level::Error,
                    "protocol.message_encode_failed",
                    &[("reason", error.to_string())],
                );
            }
            false
        }
    }
}

pub(crate) fn queue_encoded_frame(
    client: &mut Client,
    frame: &[u8],
    metrics: &mut RuntimeMetrics,
) -> bool {
    queue_encoded_frame_with_reserve(client, frame, metrics, 0)
}

pub(crate) fn queue_encoded_frame_with_reserve(
    client: &mut Client,
    frame: &[u8],
    metrics: &mut RuntimeMetrics,
    reserved_bytes: usize,
) -> bool {
    if client.close_deadline().is_some() {
        return false;
    }
    if client.output_frames.len() >= MAX_QUEUED_OUTPUT_FRAMES {
        return false;
    }
    prepare_output(client);
    if client
        .output
        .len()
        .saturating_add(frame.len())
        .saturating_add(reserved_bytes)
        > MAX_QUEUED_OUTPUT_BYTES
    {
        return false;
    }
    client.output.reserve(frame.len());
    client.output.extend_from_slice(frame);
    client
        .output_frames
        .push_back((frame.len(), Instant::now()));
    client.frames_out += 1;
    metrics.frames_out += 1;
    true
}

fn prepare_output(client: &mut Client) {
    if client.output_offset > 0 {
        let pending = client.output.len() - client.output_offset;
        client.output.copy_within(client.output_offset.., 0);
        client.output.truncate(pending);
        client.output_offset = 0;
    }
}

pub(crate) fn account_output(client: &mut Client, mut written: usize) {
    while written > 0 {
        let Some((remaining, _)) = client.output_frames.front_mut() else {
            break;
        };
        let consumed = written.min(*remaining);
        *remaining -= consumed;
        written -= consumed;
        if *remaining == 0 {
            client.output_frames.pop_front();
        }
    }
}
