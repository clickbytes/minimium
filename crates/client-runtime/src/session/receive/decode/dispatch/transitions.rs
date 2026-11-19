use super::*;

#[derive(Clone, Copy)]
pub(super) struct TransitionPreparation<'a> {
    pub id: TransitionId,
    pub source: WorldContext,
    pub target: WorldContext,
    pub map_fingerprint: u64,
    pub destination: [f32; 3],
    pub destination_mm: minimium_world::world::Position3Mm,
    pub manifest: &'a minimium_world::world::WorldManifest,
    pub clock_sample: ClockSample,
    pub environment: EnvironmentDescriptor,
}

#[derive(Clone, Copy)]
pub(super) struct TransitionCommit<'a> {
    pub id: TransitionId,
    pub server_tick: u64,
    pub context: WorldContext,
    pub map_fingerprint: u64,
    pub manifest: &'a minimium_world::world::WorldManifest,
    pub clock_sample: ClockSample,
    pub environment: EnvironmentDescriptor,
}

pub(super) fn handle_transition_prepare(
    session: &mut ClientSession,
    preparation: TransitionPreparation<'_>,
    events: &mut Vec<SessionEvent>,
) -> DispatchResult {
    let TransitionPreparation {
        id: transition_id,
        source: source_context,
        target: target_context,
        map_fingerprint,
        destination,
        destination_mm,
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
        .pending_content_for_transition(transition_id, target_context)
        .map_or(0, |content| content.map().fingerprint());
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
    let identity = super::super::super::super::transfer::PrepareIdentity {
        id: transition_id,
        source: source_context,
        target: target_context,
        map_fingerprint,
        destination,
        destination_mm,
        manifest: *manifest,
        clock_sample,
        environment,
    };
    if !session.validate_prepare_identity(identity) {
        session.disconnect(DisconnectReason::UnexpectedMessage, events);
        return DispatchResult::Stop;
    }
    let transition = (transition_id, target_context);
    match session.pending_transition {
        Some(current) if current == transition => return DispatchResult::Skip,
        Some(_) => {
            session.disconnect(DisconnectReason::UnexpectedMessage, events);
            return DispatchResult::Stop;
        }
        None => {}
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
    session.cancel_requested_transition = None;
    session.host_ready_transition = None;
    session.install_prepare_identity(identity);
    DispatchResult::Emit
}

pub(super) fn handle_transition_committed(
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
    if session.pending_transition != Some((transition_id, context)) {
        return DispatchResult::Skip;
    }
    if session.ready_transition != Some((transition_id, context)) {
        session.disconnect(DisconnectReason::UnexpectedMessage, events);
        return DispatchResult::Stop;
    }
    let expected_fingerprint = session
        .pending_content_for_transition(transition_id, context)
        .map_or(0, |content| content.map().fingerprint());
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
    if session
        .pending_content_for_transition(transition_id, context)
        .is_none()
    {
        session.disconnect(DisconnectReason::UnexpectedMessage, events);
        return DispatchResult::Stop;
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
    session.clear_transfer_lifecycle();
    let pending = session
        .pending_world_content
        .take()
        .expect("validated pending world content");
    session.active_world_content = Some(ActiveWorldContent {
        context: pending.context,
        content: pending.content,
        water_catalog: pending.water_catalog,
    });
    session.world_content_request = None;
    session.world_content_started = None;
    DispatchResult::Emit
}

pub(super) fn handle_transition_cancelled(
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
    session.clear_transfer_lifecycle();
    session.cancel_pending_environment(transition_id);
    session.pending_world_content = None;
    session.cancel_world_content_job();
    session.world_content_request = None;
    session.world_content_started = None;
    DispatchResult::Emit
}
