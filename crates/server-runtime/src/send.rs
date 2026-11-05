use crate::{
    connection::{queue_message, Client, CloseReason},
    metrics::RuntimeMetrics,
};
use minimium_protocol::ServerMessage;
use std::time::Instant;

const MAX_WRITE_BYTES_PER_CLIENT_PER_POLL: usize = 16 * 1024;
const MAX_WRITE_BYTES_PER_POLL: usize = 256 * 1024;

pub(crate) fn send_clients(
    clients: &mut [Client],
    metrics: &mut RuntimeMetrics,
    disconnected: &mut Vec<usize>,
    first_write_client: &mut usize,
) {
    let client_count = clients.len();
    let mut write_budget = MAX_WRITE_BYTES_PER_POLL;
    for offset in 0..client_count {
        let index = (*first_write_client + offset) % client_count;
        let client = &mut clients[index];
        if client.close_deadline().is_none() {
            if let Some(health) = client.pending_health_update {
                if !queue_message(client, metrics, ServerMessage::HealthUpdate(health)) {
                    metrics.dropped_messages = metrics.dropped_messages.saturating_add(1);
                } else {
                    client.pending_health_update = None;
                }
            }
        }
        metrics.max_client_input_bytes = metrics.max_client_input_bytes.max(client.input.len());
        metrics.max_client_output_bytes = metrics
            .max_client_output_bytes
            .max(client.pending_output_bytes());
        let flushed = client.flush(write_budget.min(MAX_WRITE_BYTES_PER_CLIENT_PER_POLL));
        write_budget -= flushed.written;
        metrics.bytes_out += flushed.written as u64;
        client.bytes_out += flushed.written as u64;
        if flushed.error.is_some() {
            client.disconnect_reason = Some(CloseReason::WriteError);
        }
        if flushed.error.is_some()
            || client.close_deadline().is_some_and(|deadline| {
                client.pending_output_bytes() == 0 || Instant::now() >= deadline
            })
        {
            disconnected.push(index);
        }
    }
    if client_count > 0 {
        *first_write_client = (*first_write_client + 1) % client_count;
    } else {
        *first_write_client = 0;
    }
}
