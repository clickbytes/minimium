use crate::{
    connection::{queue_close_response, Client, CloseReason},
    metrics::RuntimeMetrics,
};
use minimium_protocol::{
    decode_client, ClientMessage, ProtocolError, ServerErrorCode, ServerMessage, MAX_FRAME_BYTES,
};
use std::{
    io::{self, Read},
    time::Instant,
};

const MAX_READ_BYTES_PER_CLIENT_PER_POLL: usize = 1024;
const MAX_MESSAGES_PER_CLIENT_PER_POLL: usize = 32;

pub(crate) fn poll_clients(
    clients: &mut [Client],
    metrics: &mut RuntimeMetrics,
    messages: &mut Vec<(usize, ClientMessage, Instant)>,
    disconnected: &mut Vec<usize>,
) {
    for (index, client) in clients.iter_mut().enumerate() {
        if client.close_deadline().is_some() || client.peer_eof {
            continue;
        }
        let mut buffer = [0u8; 2048];
        let mut bytes_this_poll = 0usize;
        let mut messages_this_poll = 0usize;
        loop {
            // Drain buffered frames before reading: a previous poll may have
            // yielded with complete frames still in the input buffer.
            loop {
                match decode_client(&client.input) {
                    Ok(Some((message, frame_bytes))) => {
                        client.input.drain(..frame_bytes);
                        client.last_message_at = Instant::now();
                        messages_this_poll += 1;
                        metrics.frames_in += 1;
                        client.frames_in += 1;
                        messages.push((index, message, Instant::now()));
                        if messages_this_poll >= MAX_MESSAGES_PER_CLIENT_PER_POLL {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(ProtocolError::UnsupportedVersion(_)) => {
                        if !queue_close_response(
                            client,
                            metrics,
                            ServerMessage::Error(ServerErrorCode::Protocol),
                            Instant::now(),
                        ) {
                            disconnected.push(index);
                        }
                        break;
                    }
                    Err(_) => {
                        client.disconnect_reason = Some(CloseReason::InvalidFrame);
                        disconnected.push(index);
                        break;
                    }
                }
            }
            if client.close_deadline().is_some()
                || messages_this_poll >= MAX_MESSAGES_PER_CLIENT_PER_POLL
                || disconnected.last() == Some(&index)
            {
                break;
            }
            if client.input.len() > MAX_FRAME_BYTES {
                client.disconnect_reason = Some(CloseReason::InvalidFrame);
                disconnected.push(index);
                break;
            }
            let available =
                (MAX_READ_BYTES_PER_CLIENT_PER_POLL - bytes_this_poll).min(buffer.len());
            if available == 0 {
                break;
            }
            match client.stream.read(&mut buffer[..available]) {
                Ok(0) => {
                    // TCP half-close is legal: dispatch complete frames already
                    // decoded from this stream, then drain queued replies.
                    client.peer_eof = true;
                    break;
                }
                Ok(count) => {
                    bytes_this_poll += count;
                    metrics.bytes_in += count as u64;
                    client.bytes_in += count as u64;
                    client.input.extend_from_slice(&buffer[..count]);
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    client.disconnect_reason = Some(CloseReason::ReadError);
                    disconnected.push(index);
                    break;
                }
            }
        }
    }
}
