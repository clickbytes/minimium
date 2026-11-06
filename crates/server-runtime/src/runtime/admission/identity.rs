use super::routing::RoutingOutcome;
use crate::{
    connection::{mark_disconnected, queue_close_response, queue_message, CloseReason},
    content::ContentServer,
    runtime::ServerRuntime,
    world_inputs::WorldInput,
    MAX_PENDING_WORLD_INPUTS,
};
use minimium_core::{identity::PlayerProfileId, ids::PlayerId};
use minimium_environment::time::ClockRevision;
use minimium_protocol::{ClientMessage, ServerErrorCode, ServerMessage, WorldContext};
use minimium_world::world::{Position3Mm, WorldInstanceId};
use std::time::Instant;

pub(super) fn negotiate_content(
    runtime: &mut ServerRuntime,
    index: usize,
    message: ClientMessage,
    player_id: Option<PlayerId>,
) -> RoutingOutcome {
    let message = match message {
        ClientMessage::Presentation(capability)
            if player_id.is_none()
                && runtime.connections.clients[index].pending_profile.is_none() =>
        {
            runtime.connections.clients[index].presentation = capability.is_some();
            return RoutingOutcome::Handled;
        }
        ClientMessage::Identify(profile)
            if player_id.is_none()
                && runtime.connections.clients[index].presentation
                && !runtime.connections.clients[index].content_ready =>
        {
            if runtime.connections.clients[index].pending_profile.is_some() {
                disconnect(runtime, index, None);
                return RoutingOutcome::Handled;
            }
            runtime.connections.clients[index].pending_profile = Some(profile);
            if !queue_message(
                &mut runtime.connections.clients[index],
                &mut runtime.metrics,
                content_offer(&runtime.content),
            ) {
                disconnect(runtime, index, None);
            }
            return RoutingOutcome::Handled;
        }
        ClientMessage::ContentReady(hash) if player_id.is_none() => {
            if hash != runtime.content.hash
                || runtime.connections.clients[index].pending_profile.is_none()
            {
                disconnect(runtime, index, None);
                return RoutingOutcome::Handled;
            }
            runtime.connections.clients[index].content_ready = true;
            ClientMessage::Identify(
                runtime.connections.clients[index]
                    .pending_profile
                    .take()
                    .unwrap(),
            )
        }
        other => other,
    };
    RoutingOutcome::ContinueWith(message)
}

fn content_offer(content: &ContentServer) -> ServerMessage {
    ServerMessage::ContentOffer {
        material_hash: content.hash,
        http_port: content.port,
        climate: content.climate.clone(),
    }
}

pub(super) fn activate_profile(
    runtime: &mut ServerRuntime,
    index: usize,
    profile_id: PlayerProfileId,
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
            eprintln!("could not claim player profile: {error}");
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
    runtime.connections.clients[index].activate(player.id, profile_id);
    runtime
        .connections
        .client_indices_by_player
        .insert(player.id, index);
    let instance = player.location.instance();
    let context = match runtime.game.worlds.reserve_membership(instance, player.id) {
        Ok(context) => context,
        Err(error) => {
            eprintln!("could not reserve world membership: {error:?}");
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
                eprintln!("could not sample environment for Welcome: {error}");
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
        runtime.connections.clients[index].mark_environment_sent(
            context,
            clock_revision,
            Instant::now(),
        );
        println!("player {} connected", player.id);
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
) -> Result<(ServerMessage, ClockRevision), String> {
    let (clock_sample, environment) = runtime.environment_state_for_instance(instance, now)?;
    let clock_revision = clock_sample.revision();
    let welcome = ServerMessage::Welcome {
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
