use super::*;

pub(super) enum DispatchResult {
    Emit,
    Skip,
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
    if !prepare_manifest_geography(session, message, events) {
        return DispatchResult::Stop;
    }
    match message {
        ServerMessage::ContentOffer {
            material_hash,
            http_port,
            climate,
        } => {
            if let Err(error) = session.begin_content(*material_hash, *http_port, climate.clone()) {
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
        ServerMessage::TimeProbeReply {
            seq,
            echoed_client_send_monotonic_ms,
            server_receive_monotonic_ms,
            server_send_monotonic_ms,
            session_nonce,
        } => handle_time_probe_reply(
            session,
            *seq,
            *echoed_client_send_monotonic_ms,
            *server_receive_monotonic_ms,
            *server_send_monotonic_ms,
            *session_nonce,
            decoded_at_ms,
            events,
        ),
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

fn prepare_manifest_geography(
    session: &mut ClientSession,
    message: &ServerMessage,
    events: &mut Vec<SessionEvent>,
) -> bool {
    let planet = match message {
        ServerMessage::Welcome { manifest, .. }
        | ServerMessage::TransitionPrepare { manifest, .. }
        | ServerMessage::TransitionCommitted { manifest, .. } => {
            manifest.planet().filter(|planet| planet.mode == 1)
        }
        _ => None,
    };
    let Some(definition) = planet else {
        return true;
    };
    let package = std::env::var_os("MINIMIUM_EARTH_PACKAGE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("third_party/gebco-2026"));
    let already_loaded = session
        .prepared_geography
        .as_ref()
        .is_some_and(|geography| geography.package_digest() == definition.package_digest);
    if already_loaded {
        return true;
    }
    match minimium_asset_io::open_earth_package(&package, Some(definition.package_digest)) {
        Ok(prepared) => {
            session.prepared_geography = Some(prepared);
            true
        }
        Err(error) => {
            session.disconnect(DisconnectReason::EarthPackage(error), events);
            false
        }
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
    let expected_fingerprint = session.expected_fingerprint(context.instance(), manifest);
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
    if manifest.generator_digest()
        != minimium_world::world::GeneratorDigest::legacy_map_fingerprint(expected_fingerprint)
        && manifest.generator_digest()
            != minimium_procgen::generator_digest(manifest.topology(), manifest.generation())
    {
        session.disconnect(DisconnectReason::GeneratorMismatch, events);
        return DispatchResult::Stop;
    }
    if let Err(reason) = session.install_initial_environment(*manifest, clock_sample, environment) {
        session.disconnect(reason, events);
        return DispatchResult::Stop;
    }
    session.world_context = Some(context);
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
    let expected_fingerprint = session.expected_fingerprint(target_context.instance(), manifest);
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
        || !manifest_matches_generator(manifest)
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
        context,
        map_fingerprint,
        manifest,
        clock_sample,
        environment,
    } = commit;
    let expected_fingerprint = session.expected_fingerprint(context.instance(), manifest);
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
        || !manifest_matches_generator(manifest)
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

fn handle_time_probe_reply(
    session: &mut ClientSession,
    seq: u32,
    echoed_client_send_monotonic_ms: u64,
    server_receive_monotonic_ms: u64,
    server_send_monotonic_ms: u64,
    session_nonce: minimium_environment::time::SessionNonce,
    decoded_at_ms: u64,
    events: &mut Vec<SessionEvent>,
) -> DispatchResult {
    if let Err(reason) = session.validate_probe_nonce(session_nonce) {
        session.disconnect(reason, events);
        return DispatchResult::Stop;
    }
    session.clock.accept_probe_reply(
        seq,
        echoed_client_send_monotonic_ms,
        server_receive_monotonic_ms,
        server_send_monotonic_ms,
        decoded_at_ms,
    );
    DispatchResult::Emit
}
