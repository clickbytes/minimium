use super::routing::RoutingOutcome;
use crate::{
    authentication::{AuthenticationAttemptId, AuthenticationRequest, AuthenticationStatus},
    connection::{mark_disconnected, queue_close_response, queue_message, CloseReason},
    content::ContentServer,
    runtime::ServerRuntime,
    world_inputs::WorldInput,
    MAX_PENDING_WORLD_INPUTS,
};
use minimium_core::{identity::Principal, ids::PlayerId};
use minimium_environment::time::ClockRevision;
use minimium_protocol::{ClientMessage, ServerErrorCode, ServerMessage, WorldContext};
use minimium_world::world::{Position3Mm, WorldInstanceId};
use std::time::Instant;

pub(super) fn negotiate_content(
    runtime: &mut ServerRuntime,
    index: usize,
    message: ClientMessage,
    player_id: Option<PlayerId>,
    decoded_at: Instant,
) -> RoutingOutcome {
    let message = match message {
        ClientMessage::Presentation(capability) if player_id.is_none() => {
            let client = &mut runtime.connections.clients[index];
            if !client
                .set_presentation(capability, Instant::now() + crate::CLIENT_HANDSHAKE_TIMEOUT)
            {
                disconnect(runtime, index, None);
            }
            return RoutingOutcome::Handled;
        }
        ClientMessage::Authenticate {
            provider_id,
            credentials,
        } if player_id.is_none() => {
            if !runtime.connections.clients[index].awaits_authentication() {
                disconnect(runtime, index, None);
                return RoutingOutcome::Handled;
            }
            begin_authentication(runtime, index, provider_id, credentials);
            return RoutingOutcome::Handled;
        }
        ClientMessage::ContentReady(hash) if player_id.is_none() => {
            let principal = runtime.connections.clients[index].pending_principal();
            if hash != runtime.content.hash || principal.is_none() {
                disconnect(runtime, index, None);
                return RoutingOutcome::Handled;
            }
            activate_profile(
                runtime,
                index,
                principal.expect("principal checked above"),
                decoded_at,
            );
            return RoutingOutcome::Handled;
        }
        other => other,
    };
    RoutingOutcome::ContinueWith(message)
}

pub(super) fn has_permission(
    runtime: &mut ServerRuntime,
    index: usize,
    permission: minimium_core::identity::ServerPermission,
    action: &'static str,
) -> bool {
    let identity = runtime
        .connections
        .clients
        .get(index)
        .and_then(|client| client.identity());
    if identity.is_some_and(|identity| identity.principal.permissions().contains(permission)) {
        return true;
    }
    runtime.metrics.rejected_inputs = runtime.metrics.rejected_inputs.saturating_add(1);
    let connection_id = runtime.connections.clients[index]
        .connection_id()
        .to_string();
    crate::diagnostics::emit(
        &runtime.server_logger,
        minimium_diagnostics::Level::Warn,
        "server.permission_denied",
        &[("action", action.to_owned()), ("connection", connection_id)],
    );
    false
}

fn begin_authentication(
    runtime: &mut ServerRuntime,
    index: usize,
    provider_id: String,
    credentials: Vec<u8>,
) {
    if provider_id.len() > crate::authentication::MAX_PROVIDER_ID_BYTES
        || credentials.len() > crate::authentication::MAX_CREDENTIAL_BYTES
    {
        disconnect(runtime, index, None);
        return;
    }
    let Some(attempt) = AuthenticationAttemptId::new(runtime.next_authentication_attempt) else {
        crate::diagnostics::emit(
            &runtime.server_logger,
            minimium_diagnostics::Level::Error,
            "server.authentication_attempt_id_exhausted",
            &[],
        );
        disconnect(runtime, index, None);
        return;
    };
    let Some(next) = runtime.next_authentication_attempt.checked_add(1) else {
        runtime.next_authentication_attempt = 0;
        disconnect(runtime, index, None);
        return;
    };
    runtime.next_authentication_attempt = next;
    if !runtime.connections.clients[index].awaits_authentication() {
        disconnect(runtime, index, None);
        return;
    }
    let request = AuthenticationRequest {
        attempt_id: attempt,
        provider_id,
        credentials,
    };
    if let Err(error) = runtime.authentication_provider.begin(request) {
        crate::diagnostics::emit(
            &runtime.server_logger,
            minimium_diagnostics::Level::Warn,
            error.code(),
            &[("connection", connection_id(runtime, index))],
        );
        reject_authentication(runtime, index);
        return;
    }
    runtime.connections.clients[index].begin_authentication(
        attempt,
        Instant::now() + crate::CLIENT_AUTHENTICATION_TIMEOUT,
    );
}

pub(super) fn poll_authentication(runtime: &mut ServerRuntime) {
    let now = Instant::now();
    for index in 0..runtime.connections.clients.len() {
        let Some((attempt, deadline)) = runtime.connections.clients[index].authentication_attempt()
        else {
            continue;
        };
        if now >= deadline {
            runtime.authentication_provider.cancel(attempt);
            crate::diagnostics::emit(
                &runtime.server_logger,
                minimium_diagnostics::Level::Warn,
                "server.authentication_timeout",
                &[("connection", connection_id(runtime, index))],
            );
            reject_authentication(runtime, index);
            continue;
        }
        match runtime.authentication_provider.poll(attempt) {
            AuthenticationStatus::Pending => {}
            AuthenticationStatus::Rejected(error) => {
                runtime.authentication_provider.cancel(attempt);
                crate::diagnostics::emit(
                    &runtime.server_logger,
                    minimium_diagnostics::Level::Warn,
                    error.code(),
                    &[("connection", connection_id(runtime, index))],
                );
                reject_authentication(runtime, index);
            }
            AuthenticationStatus::Authenticated(principal) => {
                let deadline = now + crate::CLIENT_CONTENT_TIMEOUT;
                if !runtime.connections.clients[index].authentication_complete(principal, deadline)
                {
                    runtime.authentication_provider.cancel(attempt);
                    disconnect(runtime, index, None);
                    continue;
                }
                if runtime.connections.clients[index]
                    .presentation_capability()
                    .is_none()
                {
                    activate_profile(runtime, index, principal, now);
                } else if !queue_message(
                    &mut runtime.connections.clients[index],
                    &mut runtime.metrics,
                    content_offer(&runtime.content, &runtime.climate),
                ) {
                    disconnect(runtime, index, None);
                }
            }
        }
    }
}

fn reject_authentication(runtime: &mut ServerRuntime, index: usize) {
    if index >= runtime.connections.clients.len() {
        return;
    }
    if !queue_close_response(
        &mut runtime.connections.clients[index],
        &mut runtime.metrics,
        ServerMessage::Error(ServerErrorCode::Busy),
        Instant::now(),
    ) {
        disconnect(runtime, index, None);
    }
}

fn connection_id(runtime: &ServerRuntime, index: usize) -> String {
    runtime.connections.clients[index]
        .connection_id()
        .to_string()
}

fn content_offer(
    content: &ContentServer,
    climate: &minimium_environment::climate::ClimateScenario,
) -> ServerMessage {
    ServerMessage::ContentOffer {
        material_hash: content.hash,
        http_port: content.port,
        climate: climate.clone(),
    }
}

pub(super) fn activate_profile(
    runtime: &mut ServerRuntime,
    index: usize,
    principal: Principal,
    decoded_at: Instant,
) {
    if runtime.operations.player_store.active_count() >= runtime.max_players
        || runtime.game.pending_world_inputs.len() + runtime.connections.clients.len()
            >= MAX_PENDING_WORLD_INPUTS
    {
        if !queue_close_response(
            &mut runtime.connections.clients[index],
            &mut runtime.metrics,
            ServerMessage::Error(ServerErrorCode::ServerFull),
            Instant::now(),
        ) {
            disconnect(runtime, index, None);
        }
        return;
    }
    let profile_id = principal.profile();
    let player_policy = runtime.movement_policy;
    if runtime.game.pending_departures.contains(&profile_id) {
        if !queue_close_response(
            &mut runtime.connections.clients[index],
            &mut runtime.metrics,
            ServerMessage::Error(ServerErrorCode::Busy),
            Instant::now(),
        ) {
            disconnect(runtime, index, None);
        }
        return;
    }

    let player = match runtime.operations.player_store.claim(
        profile_id,
        &mut runtime.operations.next_player,
        runtime.map.spawn(),
        runtime.map.spawn_position_mm(),
        runtime.game.worlds.primary_id(),
        runtime.game.worlds.primary().world.gravity(),
    ) {
        Ok(player) => player,
        Err(error) => {
            crate::diagnostics::emit(
                &runtime.world_logger,
                minimium_diagnostics::Level::Warn,
                "server.profile_claim_failed",
                &[("reason", error.to_string())],
            );
            if !queue_close_response(
                &mut runtime.connections.clients[index],
                &mut runtime.metrics,
                ServerMessage::Error(ServerErrorCode::Busy),
                Instant::now(),
            ) {
                disconnect(runtime, index, None);
            }
            return;
        }
    };
    runtime
        .game
        .player_movement_policies
        .insert(player.id, player_policy);
    runtime.connections.clients[index].activate(player.id, principal);
    runtime
        .connections
        .client_indices_by_player
        .insert(player.id, index);
    let instance = player.location.instance();
    let context = match runtime.game.worlds.reserve_membership(instance, player.id) {
        Ok(context) => context,
        Err(error) => {
            crate::diagnostics::emit(
                &runtime.server_logger,
                minimium_diagnostics::Level::Error,
                "world.membership_reservation_failed",
                &[
                    ("player_id", player.id.to_string()),
                    ("reason", format!("{error:?}")),
                ],
            );
            disconnect(runtime, index, None);
            return;
        }
    };
    runtime.game.pending_world_inputs.push((
        WorldInput::Join {
            id: player.id,
            instance,
            name: player.name.as_str().to_owned(),
            position: player.position,
            position_mm: player.location.position(),
            health: player.health,
        },
        Some(decoded_at),
    ));

    let welcome_player = WelcomePlayerData {
        id: player.id,
        health: player.health,
        position: player.position,
        position_mm: player.location.position(),
    };
    let now = Instant::now();
    let (welcome, clock_revision) =
        match build_welcome_message(runtime, welcome_player, instance, context, now) {
            Ok(welcome) => welcome,
            Err(error) => {
                crate::diagnostics::emit(
                    &runtime.server_logger,
                    minimium_diagnostics::Level::Error,
                    "server.welcome_environment_failed",
                    &[
                        ("player_id", player.id.to_string()),
                        ("error_code", error.code().to_owned()),
                        ("reason", error.to_string()),
                    ],
                );
                disconnect(runtime, index, Some(CloseReason::InvalidSequence));
                return;
            }
        };
    if !queue_message(
        &mut runtime.connections.clients[index],
        &mut runtime.metrics,
        welcome,
    ) {
        runtime.metrics.dropped_messages = runtime.metrics.dropped_messages.saturating_add(1);
        disconnect(runtime, index, Some(CloseReason::Backpressure));
    } else {
        if !queue_message(
            &mut runtime.connections.clients[index],
            &mut runtime.metrics,
            ServerMessage::MovementPolicy(player_policy),
        ) {
            runtime.metrics.dropped_messages = runtime.metrics.dropped_messages.saturating_add(1);
            disconnect(runtime, index, Some(CloseReason::Backpressure));
            return;
        }
        runtime.connections.clients[index].mark_environment_sent(
            context,
            clock_revision,
            Instant::now(),
        );
        crate::diagnostics::emit(
            &runtime.server_logger,
            minimium_diagnostics::Level::Info,
            "server.player_connected",
            &[("player_id", player.id.to_string())],
        );
    }
}

struct WelcomePlayerData {
    id: PlayerId,
    health: u16,
    position: [f32; 3],
    position_mm: Position3Mm,
}

fn build_welcome_message(
    runtime: &mut ServerRuntime,
    player: WelcomePlayerData,
    instance: WorldInstanceId,
    context: WorldContext,
    now: Instant,
) -> Result<(ServerMessage, ClockRevision), crate::environment::EnvironmentClockError> {
    let (clock_sample, environment) = runtime.environment_state_for_instance(instance, now)?;
    let clock_revision = clock_sample.revision();
    let welcome = ServerMessage::Welcome {
        server_tick: runtime.scheduler.simulation_tick,
        id: player.id,
        map_fingerprint: runtime
            .game
            .worlds
            .map_fingerprint_for_instance(instance)
            .expect("reserved membership references a loaded world"),
        health: player.health,
        position: player.position,
        position_mm: player.position_mm,
        context,
        manifest: runtime
            .game
            .worlds
            .manifest_for_player(player.id)
            .expect("reserved membership has a manifest"),
        transition_id: None,
        clock_sample,
        environment,
    };
    Ok((welcome, clock_revision))
}

fn disconnect(runtime: &mut ServerRuntime, index: usize, reason: Option<CloseReason>) {
    mark_disconnected(
        index,
        &mut runtime.connections.clients[index],
        &mut runtime.connections.disconnected,
        reason,
    );
}
