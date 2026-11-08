mod manifest;
mod time_probe;
use super::*;
use manifest::manifest_matches_content;
use time_probe::handle_time_probe_reply;

pub(super) enum DispatchResult {
    Emit,
    Skip,
    Pending,
    Stop,
}

#[derive(Clone, Copy)]
struct TransitionPreparation<'a> {
    id: TransitionId,
    source: WorldContext,
    target: WorldContext,
    map_fingerprint: u64,
    manifest: &'a minimium_world::world::WorldManifest,
    clock_sample: ClockSample,
    environment: EnvironmentDescriptor,
}

#[derive(Clone, Copy)]
struct TransitionCommit<'a> {
    id: TransitionId,
    server_tick: u64,
    context: WorldContext,
    map_fingerprint: u64,
    manifest: &'a minimium_world::world::WorldManifest,
    clock_sample: ClockSample,
    environment: EnvironmentDescriptor,
}

pub(super) fn dispatch_message(
    session: &mut ClientSession,
    message: &ServerMessage,
    decoded_at_ms: u64,
    events: &mut Vec<SessionEvent>,
) -> DispatchResult {
    match message {
        ServerMessage::Welcome {
            context,
            manifest,
            map_fingerprint,
            ..
        } => {
            if session.state != SessionState::AwaitingWelcome {
                session.disconnect(DisconnectReason::UnexpectedMessage, events);
                return DispatchResult::Stop;
            }
            if manifest.content_revision() != context.content_revision() {
                session.disconnect(DisconnectReason::GeneratorMismatch, events);
                return DispatchResult::Stop;
            }
            if !manifest_matches_content(manifest, *map_fingerprint) {
                session.disconnect(DisconnectReason::GeneratorMismatch, events);
                return DispatchResult::Stop;
            }
        }
        ServerMessage::TransitionPrepare {
            source_context,
            target_context,
            manifest,
            map_fingerprint,
            ..
        } => {
            if session.state != SessionState::Connected {
                session.disconnect(DisconnectReason::UnexpectedMessage, events);
                return DispatchResult::Stop;
            }
            if Some(*source_context) != session.world_context {
                return DispatchResult::Skip;
            }
            if manifest.content_revision() != target_context.content_revision()
                || !manifest_matches_content(manifest, *map_fingerprint)
            {
                session.disconnect(DisconnectReason::GeneratorMismatch, events);
                return DispatchResult::Stop;
            }
        }
        ServerMessage::TransitionCommitted {
            transition_id,
            context,
            manifest,
            map_fingerprint,
            ..
        } => {
            if session.pending_transition != Some((*transition_id, *context)) {
                return DispatchResult::Skip;
            }
            if manifest.content_revision() != context.content_revision()
                || !manifest_matches_content(manifest, *map_fingerprint)
            {
                session.disconnect(DisconnectReason::GeneratorMismatch, events);
                return DispatchResult::Stop;
            }
        }
        _ => {}
    }
    let world_content = match message {
        ServerMessage::Welcome {
            context,
            manifest,
            map_fingerprint,
            ..
        }
        | ServerMessage::TransitionPrepare {
            target_context: context,
            manifest,
            map_fingerprint,
            ..
        } => Some((context.instance(), *manifest, *map_fingerprint)),
        ServerMessage::TransitionCommitted {
            context,
            manifest,
            map_fingerprint,
            ..
        } => Some((context.instance(), *manifest, *map_fingerprint)),
        _ => None,
    };
    if let Some((instance, manifest, fingerprint)) = world_content {
        match session.prepare_world_content(instance, manifest, fingerprint) {
            Ok(super::super::WorldContentProgress::Ready) => {}
            Ok(super::super::WorldContentProgress::Pending) => return DispatchResult::Pending,
            Err(error) => {
                session.log_failure(&error);
                session.disconnect(DisconnectReason::WorldContent(error), events);
                return DispatchResult::Stop;
            }
        }
    }
    match message {
        ServerMessage::Bolid { context, .. } => {
            if session.state == SessionState::Connected
                && session.world_context == Some(*context)
                && session.pending_transition.is_none()
            {
                DispatchResult::Emit
            } else {
                DispatchResult::Skip
            }
        }
        ServerMessage::EntitySyncBegin { .. }
        | ServerMessage::EntitySyncRecord { .. }
        | ServerMessage::EntitySyncComplete { .. } => {
            match session.apply_replication_message(message) {
                Ok(_) => DispatchResult::Skip,
                Err(error) => {
                    if let Some(logger) = &session.diagnostic_logger {
                        crate::diagnostics::emit(
                            Some(logger),
                            minimium_diagnostics::Level::Error,
                            "client.replication_rejected",
                            [crate::diagnostics::field("reason", format!("{error:?}"))],
                        );
                    }
                    session.disconnect(DisconnectReason::Replication(error), events);
                    DispatchResult::Stop
                }
            }
        }
        ServerMessage::ContentOffer {
            material_hash,
            http_port,
            climate,
        } => {
            if let Err(error) = session.begin_content(*material_hash, *http_port, climate.clone()) {
                session.log_failure(&error);
                session.disconnect(DisconnectReason::Materials(error), events);
                DispatchResult::Stop
            } else {
                DispatchResult::Skip
            }
        }
        ServerMessage::Welcome {
            map_fingerprint,
            context,
            manifest,
            clock_sample,
            environment,
            ..
        } => handle_welcome(
            session,
            *map_fingerprint,
            *context,
            manifest,
            *clock_sample,
            *environment,
            events,
        ),
        ServerMessage::TransitionPrepare {
            transition_id,
            source_context,
            target_context,
            map_fingerprint,
            manifest,
            clock_sample,
            environment,
            ..
        } => handle_transition_prepare(
            session,
            TransitionPreparation {
                id: *transition_id,
                source: *source_context,
                target: *target_context,
                map_fingerprint: *map_fingerprint,
                manifest,
                clock_sample: *clock_sample,
                environment: *environment,
            },
            events,
        ),
        ServerMessage::TransitionCommitted {
            transition_id,
            server_tick,
            context,
            map_fingerprint,
            manifest,
            clock_sample,
            environment,
            ..
        } => handle_transition_committed(
            session,
            TransitionCommit {
                id: *transition_id,
                server_tick: *server_tick,
                context: *context,
                map_fingerprint: *map_fingerprint,
                manifest,
                clock_sample: *clock_sample,
                environment: *environment,
            },
            events,
        ),
        ServerMessage::TransitionCancelled {
            transition_id,
            context,
        } => handle_transition_cancelled(session, *transition_id, *context),
        ServerMessage::EnvironmentSync {
            context,
            clock_sample,
            environment,
        } => handle_environment_sync(session, *context, *clock_sample, *environment, events),
        ServerMessage::TimeProbeReply { .. } => {
            handle_time_probe_reply(session, message, decoded_at_ms, events)
        }
        ServerMessage::Error(code) if session.state == SessionState::AwaitingWelcome => {
            session.disconnect(DisconnectReason::Rejected(*code), events);
            DispatchResult::Stop
        }
        ServerMessage::Bye if session.state == SessionState::Connected => {
            session.disconnect(DisconnectReason::ServerClosed, events);
            DispatchResult::Stop
        }
        _ if session.state != SessionState::Connected => {
            session.disconnect(DisconnectReason::UnexpectedMessage, events);
            DispatchResult::Stop
        }
        _ => DispatchResult::Emit,
    }
}

fn handle_welcome(
    session: &mut ClientSession,
    map_fingerprint: u64,
    context: WorldContext,
    manifest: &minimium_world::world::WorldManifest,
    clock_sample: ClockSample,
    environment: EnvironmentDescriptor,
    events: &mut Vec<SessionEvent>,
) -> DispatchResult {
    if session.state != SessionState::AwaitingWelcome {
        session.disconnect(DisconnectReason::UnexpectedMessage, events);
        return DispatchResult::Stop;
    }
    let expected_fingerprint = session
        .resolved_world_content(context.instance())
        .map_or(0, |content| content.map.fingerprint());
    if map_fingerprint != expected_fingerprint {
        session.disconnect(
            DisconnectReason::MapMismatch {
                expected: expected_fingerprint,
                actual: map_fingerprint,
            },
            events,
        );
        return DispatchResult::Stop;
    }
    if manifest.content_revision() != context.content_revision() {
        session.disconnect(DisconnectReason::GeneratorMismatch, events);
        return DispatchResult::Stop;
    }
    if !manifest_matches_content(manifest, expected_fingerprint) {
        session.disconnect(DisconnectReason::GeneratorMismatch, events);
        return DispatchResult::Stop;
    }
    if let Err(reason) = session.install_initial_environment(*manifest, clock_sample, environment) {
        session.disconnect(reason, events);
        return DispatchResult::Stop;
    }
    session.world_context = Some(context);
    session.clear_replicas();
    session.next_input_seq = 1;
    session.next_target_tick = session.server_tick.saturating_add(2).max(1);
    session.pending_transition = None;
    session.ready_transition = None;
    session.state = SessionState::Connected;
    session.state_since = Instant::now();
    session.last_heartbeat_at = session.state_since;
    DispatchResult::Emit
}

fn handle_transition_prepare(
    session: &mut ClientSession,
    preparation: TransitionPreparation<'_>,
    events: &mut Vec<SessionEvent>,
) -> DispatchResult {
    let TransitionPreparation {
        id: transition_id,
        source: source_context,
        target: target_context,
        map_fingerprint,
        manifest,
        clock_sample,
        environment,
    } = preparation;
    if session.state != SessionState::Connected {
        session.disconnect(DisconnectReason::UnexpectedMessage, events);
        return DispatchResult::Stop;
    }
    if Some(source_context) != session.world_context {
        return DispatchResult::Skip;
    }
    let expected_fingerprint = session
        .resolved_world_content(target_context.instance())
        .map_or(0, |content| content.map.fingerprint());
    if map_fingerprint != expected_fingerprint {
        session.disconnect(
            DisconnectReason::MapMismatch {
                expected: expected_fingerprint,
                actual: map_fingerprint,
            },
            events,
        );
        return DispatchResult::Stop;
    }
    if manifest.content_revision() != target_context.content_revision()
        || !manifest_matches_content(manifest, map_fingerprint)
    {
        session.disconnect(DisconnectReason::GeneratorMismatch, events);
        return DispatchResult::Stop;
    }
    let transition = (transition_id, target_context);
    if session
        .pending_transition
        .is_some_and(|current| current != transition)
    {
        session.disconnect(DisconnectReason::UnexpectedMessage, events);
        return DispatchResult::Stop;
    }
    if let Err(reason) = session.prepare_environment(
        transition_id,
        target_context,
        *manifest,
        clock_sample,
        environment,
    ) {
        session.disconnect(reason, events);
        return DispatchResult::Stop;
    }
    session.pending_transition = Some(transition);
    session.ready_transition = None;
    DispatchResult::Emit
}

fn handle_transition_committed(
    session: &mut ClientSession,
    commit: TransitionCommit<'_>,
    events: &mut Vec<SessionEvent>,
) -> DispatchResult {
    let TransitionCommit {
        id: transition_id,
        server_tick,
        context,
        map_fingerprint,
        manifest,
        clock_sample,
        environment,
    } = commit;
    let expected_fingerprint = session
        .resolved_world_content(context.instance())
        .map_or(0, |content| content.map.fingerprint());
    if map_fingerprint != expected_fingerprint {
        session.disconnect(
            DisconnectReason::MapMismatch {
                expected: expected_fingerprint,
                actual: map_fingerprint,
            },
            events,
        );
        return DispatchResult::Stop;
    }
    if manifest.content_revision() != context.content_revision()
        || !manifest_matches_content(manifest, map_fingerprint)
    {
        session.disconnect(DisconnectReason::GeneratorMismatch, events);
        return DispatchResult::Stop;
    }
    if session.pending_transition != Some((transition_id, context)) {
        return DispatchResult::Skip;
    }
    if let Err(reason) =
        session.commit_environment(transition_id, context, *manifest, clock_sample, environment)
    {
        session.disconnect(reason, events);
        return DispatchResult::Stop;
    }
    session.world_context = Some(context);
    session.clear_replicas();
    session.server_tick = server_tick;
    session.next_input_seq = 1;
    session.next_target_tick = session.server_tick.saturating_add(2).max(1);
    session.pending_transition = None;
    session.ready_transition = None;
    // Keep queued bytes: a partially sent frame must be completed. The server
    // rejects stale-context inputs; heartbeat and clock probes still apply.
    DispatchResult::Emit
}

fn handle_transition_cancelled(
    session: &mut ClientSession,
    transition_id: TransitionId,
    context: WorldContext,
) -> DispatchResult {
    if !session
        .pending_transition
        .is_some_and(|(pending, _)| pending == transition_id)
        || Some(context) != session.world_context
    {
        return DispatchResult::Skip;
    }
    session.pending_transition = None;
    session.ready_transition = None;
    session.cancel_pending_environment(transition_id);
    DispatchResult::Emit
}

fn handle_environment_sync(
    session: &mut ClientSession,
    context: WorldContext,
    clock_sample: ClockSample,
    environment: EnvironmentDescriptor,
    events: &mut Vec<SessionEvent>,
) -> DispatchResult {
    if session.state != SessionState::Connected {
        session.disconnect(DisconnectReason::UnexpectedMessage, events);
        return DispatchResult::Stop;
    }
    match session.apply_environment_sync(context, clock_sample, environment) {
        Ok(true) => DispatchResult::Emit,
        Ok(false) => DispatchResult::Skip,
        Err(reason) => {
            session.disconnect(reason, events);
            DispatchResult::Stop
        }
    }
}
