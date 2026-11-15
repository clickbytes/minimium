use super::routing::RoutingOutcome;
use crate::{
    connection::{mark_disconnected, queue_close_response, queue_message},
    runtime::ServerRuntime,
};
use minimium_core::ids::PlayerId;
use minimium_protocol::{ClientMessage, ServerErrorCode, ServerMessage};
use std::time::{Duration, Instant};

pub(super) fn handle_message(
    runtime: &mut ServerRuntime,
    index: usize,
    message: ClientMessage,
    decoded_at: Instant,
    player_id: Option<PlayerId>,
) -> RoutingOutcome {
    match message {
        ClientMessage::SkyView {
            context,
            direction,
            night,
        } if player_id.is_some() => {
            let player = player_id.expect("guarded above");
            if runtime.game.worlds.context_matches(player, context) {
                runtime
                    .sky_views
                    .insert(player, (context, direction, night, decoded_at));
            } else {
                runtime.metrics.rejected_inputs = runtime.metrics.rejected_inputs.saturating_add(1);
            }
            RoutingOutcome::Handled
        }
        ClientMessage::Ping if player_id.is_some() => {
            if !queue_message(
                &mut runtime.connections.clients[index],
                &mut runtime.metrics,
                ServerMessage::Pong,
            ) {
                runtime.metrics.dropped_messages =
                    runtime.metrics.dropped_messages.saturating_add(1);
            }
            RoutingOutcome::Handled
        }
        ClientMessage::TimeProbe {
            seq,
            client_send_monotonic_ms,
        } if player_id.is_some() => {
            handle_time_probe(runtime, index, seq, client_send_monotonic_ms, decoded_at);
            RoutingOutcome::Handled
        }
        ClientMessage::ReplicationAck {
            context,
            sync_id,
            revision,
            accepted,
        } if player_id.is_some() => {
            let client = &mut runtime.connections.clients[index];
            let matches_pending = client.entity_sync.as_ref().is_some_and(|pending| {
                pending.context == context
                    && pending.sync_id == sync_id
                    && pending.target_revision == revision
                    && pending.complete_sent
            });
            if matches_pending {
                if accepted {
                    if let Some(pending) = client.entity_sync.take() {
                        client.entity_revision = pending.target_revision;
                        client.entity_sync_initialized = true;
                        client.replicated_components = pending.visible_components;
                    }
                } else {
                    client.entity_sync = None;
                    client.entity_revision = 0;
                    client.entity_sync_initialized = false;
                    client.replicated_components.clear();
                }
            } else {
                runtime.metrics.rejected_inputs = runtime.metrics.rejected_inputs.saturating_add(1);
            }
            RoutingOutcome::Handled
        }
        ClientMessage::Quit if player_id.is_some() => {
            if !queue_close_response(
                &mut runtime.connections.clients[index],
                &mut runtime.metrics,
                ServerMessage::Bye,
                Instant::now(),
            ) {
                mark_disconnected(
                    index,
                    &mut runtime.connections.clients[index],
                    &mut runtime.connections.disconnected,
                    None,
                );
            }
            RoutingOutcome::Handled
        }
        other => RoutingOutcome::ContinueWith(other),
    }
}

fn handle_time_probe(
    runtime: &mut ServerRuntime,
    index: usize,
    seq: u32,
    client_send_monotonic_ms: u64,
    decoded_at: Instant,
) {
    runtime.metrics.clock_probe_requests = runtime.metrics.clock_probe_requests.saturating_add(1);
    let now = Instant::now();
    if !runtime.connections.clients[index].accept_time_probe(now) {
        runtime.metrics.clock_probe_rejected =
            runtime.metrics.clock_probe_rejected.saturating_add(1);
        runtime.metrics.rejected_inputs = runtime.metrics.rejected_inputs.saturating_add(1);
        let may_reply = runtime.connections.clients[index]
            .last_time_probe_rejection_at
            .is_none_or(|last| now.saturating_duration_since(last) >= Duration::from_secs(1));
        if may_reply
            && queue_message(
                &mut runtime.connections.clients[index],
                &mut runtime.metrics,
                ServerMessage::Error(ServerErrorCode::RateLimit),
            )
        {
            runtime.connections.clients[index].last_time_probe_rejection_at = Some(now);
        }
        return;
    }
    let server_receive_monotonic_ms = match runtime.monotonic_ms(decoded_at) {
        Ok(value) => value,
        Err(error) => {
            crate::diagnostics::emit(
                &runtime.server_logger,
                minimium_diagnostics::Level::Error,
                "server.clock_probe_timestamp_failed",
                &[("reason", error.to_string())],
            );
            reject_clock_probe(runtime);
            return;
        }
    };
    let send_at = Instant::now();
    let server_send_monotonic_ms = match runtime.monotonic_ms(send_at) {
        Ok(value) => value,
        Err(error) => {
            crate::diagnostics::emit(
                &runtime.server_logger,
                minimium_diagnostics::Level::Error,
                "server.clock_probe_timestamp_failed",
                &[("reason", error.to_string())],
            );
            reject_clock_probe(runtime);
            return;
        }
    };
    let sample = match runtime.environment_clock.sample(server_send_monotonic_ms) {
        Ok(sample) => sample,
        Err(error) => {
            crate::diagnostics::emit(
                &runtime.environment_logger,
                minimium_diagnostics::Level::Error,
                "environment.clock_probe_sample_failed",
                &[
                    ("error_code", error.code().to_owned()),
                    ("reason", error.to_string()),
                ],
            );
            reject_clock_probe(runtime);
            return;
        }
    };
    if queue_message(
        &mut runtime.connections.clients[index],
        &mut runtime.metrics,
        ServerMessage::TimeProbeReply {
            seq,
            echoed_client_send_monotonic_ms: client_send_monotonic_ms,
            server_receive_monotonic_ms: server_receive_monotonic_ms.get(),
            server_send_monotonic_ms: server_send_monotonic_ms.get(),
            session_nonce: sample.session_nonce(),
        },
    ) {
        runtime.metrics.clock_probe_accepted =
            runtime.metrics.clock_probe_accepted.saturating_add(1);
    } else {
        runtime.metrics.clock_probe_rejected =
            runtime.metrics.clock_probe_rejected.saturating_add(1);
        runtime.metrics.dropped_messages = runtime.metrics.dropped_messages.saturating_add(1);
    }
}

fn reject_clock_probe(runtime: &mut ServerRuntime) {
    runtime.metrics.clock_probe_rejected = runtime.metrics.clock_probe_rejected.saturating_add(1);
    runtime.metrics.rejected_inputs = runtime.metrics.rejected_inputs.saturating_add(1);
}
