use super::*;
use minimium_protocol::ClientWorldInput;
use minimium_world::world::{
    GeneratorDigest, MembershipEpoch, WorldContentRevision, WorldInstanceId,
};

fn context(epoch: u64) -> WorldContext {
    WorldContext::new(
        WorldInstanceId::new(1).unwrap(),
        WorldContentRevision::new(1).unwrap(),
        MembershipEpoch::new(epoch).unwrap(),
    )
}

fn connected_session(context: WorldContext) -> ClientSession {
    let now = Instant::now();
    ClientSession {
        diagnostic_logger: None,
        content: Default::default(),
        stream: None,
        state: SessionState::Connected,
        world_context: Some(context),
        server_tick: 1,
        next_input_seq: 1,
        next_target_tick: 3,
        session_id: 0,
        world_content_resolver: None,
        world_content_worker: None,
        world_content_worker_ticket: None,
        world_content_request: None,
        next_world_content_request_id: 1,
        world_content_cancel: crate::ContentCancellation::default(),
        world_content_started: None,
        deferred_world_message: None,
        resolved_worlds: HashMap::new(),
        replica_schemas: HashMap::new(),
        replica_assembly: None,
        replicas: crate::ReplicatedEntities::default(),
        pending_transition: None,
        ready_transition: None,
        clock_sample: None,
        environment_descriptor: None,
        pending_environment: None,
        clock: clock::ClientClock::new(now, None),
        state_since: now,
        last_heartbeat_at: now,
        pending_ping_at: None,
        pending_ping_end: None,
        receive_buffer: Vec::new(),
        send_buffer: Vec::new(),
        send_offset: 0,
        metrics: SessionMetrics::default(),
        prepared_geography: None,
        content_cache_dir: None,
        content_cache_limit_bytes: minimium_asset_io::DEFAULT_CACHE_LIMIT_BYTES,
    }
}

#[test]
fn world_inputs_from_a_previous_membership_epoch_are_rejected() {
    let current = context(8);
    let old = context(7);
    let mut session = connected_session(current);
    let input = ClientMessage::WorldInput {
        context: old,
        input: ClientWorldInput::InteractEntity {
            entity: minimium_world::entity::EntityId::new(1).unwrap(),
        },
    };
    assert_eq!(session.queue(&input), Err(QueueError::StaleWorldContext));

    let current_input = ClientMessage::WorldInput {
        context: current,
        input: ClientWorldInput::InteractEntity {
            entity: minimium_world::entity::EntityId::new(1).unwrap(),
        },
    };
    assert_eq!(session.queue(&current_input), Ok(()));
}

#[test]
fn client_rejects_a_manifest_with_an_unknown_generator_digest() {
    let generated = minimium_game_sample::generate_demo_worlds().unwrap();
    let manifest = generated[0].manifest;
    assert!(manifest_matches_generator(&manifest));
    let mismatched = minimium_world::world::WorldManifest::new(
        manifest.definition_id(),
        manifest.topology(),
        manifest.generation(),
        manifest.content_revision(),
        GeneratorDigest::new([0xff; 32]),
    );
    assert!(!manifest_matches_generator(&mismatched));
}
