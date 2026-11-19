//! Public CPU application flow: session dispatch, content completion,
//! prediction, server reconciliation and an immutable client view. Compile only.
#[path = "support/local_content_jobs.rs"]
mod local_content_jobs;
#[path = "support/local_transport.rs"]
mod local_transport;

use local_content_jobs::LocalContentJobs;
use local_transport::LocalTransport;
use minimium_client_runtime::{
    world_readiness::{CollisionSnapshot, WorldReadiness, WorldReadinessOwner},
    ClientApplicationConfig, ClientApplicationCore, ClientAuthentication, ClientSession,
    ClientStepInput, ContentJobRequest, ContentJobResult, ContentJobTicket, ContentJobsPort,
    ContentJobsSession, PreparedWorldContent, ResolvedWorldContent,
};
use minimium_environment::{
    time::{
        ClockId, ClockMode, ClockRevision, ClockSample, MonotonicMs, OfflinePolicy, SessionNonce,
        TimeScalePpm, WorldTimeMs,
    },
    AstronomyModelRevision, CatalogueId, CatalogueSha256, EnvironmentDescriptor,
    EnvironmentProfile,
};
use minimium_protocol::{
    ClientMessage, ClientWorldInput, RemotePlayerIds, RemotePlayers, ServerMessage, WorldContext,
};
use minimium_simulation::tools::{ToolDefinition, ToolProfile};
use minimium_world::{
    items::ToolKind,
    terrain::TerrainMaterialSet,
    world::{
        GenerationRevision, GenerationSpec, GeneratorDigest, MapBox, MembershipEpoch, Position3Mm,
        WorldContentRevision, WorldDefinitionId, WorldInstanceId, WorldManifest, WorldMap,
        WorldSeed, WorldTopology,
    },
};
use std::{
    num::NonZeroU64,
    sync::Arc,
    time::{Duration, Instant},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let base = Instant::now();
    let now = time_at(base, 0);
    let map = Arc::new(WorldMap::from_parts(
        [0.0, 2.0, 0.0],
        vec![MapBox {
            center: [0.0, -0.5, 0.0],
            size: [256.0, 1.0, 256.0],
            color: [0.3; 3],
            solid: true,
        }],
        Vec::new(),
    )?);
    let instance = WorldInstanceId::new(1).unwrap();
    let manifest = WorldManifest::new(
        WorldDefinitionId::new(1).unwrap(),
        WorldTopology::Dungeon,
        GenerationSpec::new(WorldSeed::new(7), GenerationRevision::new(6).unwrap(), 1).unwrap(),
        WorldContentRevision::new(1).unwrap(),
        GeneratorDigest::authored_map_fingerprint(map.fingerprint()),
    );
    let context = WorldContext::new(
        instance,
        manifest.content_revision(),
        MembershipEpoch::new(1).unwrap(),
    );
    let terrain_profile = minimium_procgen::TerrainMaterialProfile::new(1500)
        .ok_or("invalid terrain material profile")?
        .bind(manifest, map.fingerprint());
    let terrain = minimium_procgen::TerrainInputs::new(
        minimium_procgen::PreparedTerrainSource::new(
            instance,
            manifest,
            &map,
            None,
            terrain_profile,
            None,
        )?,
        minimium_procgen::TerrainOverlaySnapshot::empty_for(instance),
    )?;

    let jobs = Arc::new(LocalContentJobs::default());
    let content_peer = jobs.peer();
    let jobs_port: Arc<dyn ContentJobsPort> = jobs.clone();
    let transport = LocalTransport::default();
    let peer = transport.peer();
    let mut connection = minimium_client_runtime::ConnectionController::new(now)?;
    connection.set_content_jobs(jobs_port)?;
    let session = ClientSession::new(
        Box::new(transport),
        ClientAuthentication::new("cpu-example", b"bounded local credential".to_vec())?,
        ContentJobsSession::new(NonZeroU64::new(1).unwrap(), jobs.clone()),
        now,
    )?;
    let mut worlds = std::collections::HashMap::new();
    worlds.insert(instance, (*map).clone());
    let mut manifests = std::collections::HashMap::new();
    manifests.insert(instance, manifest);
    let mut core = ClientApplicationCore::new(
        connection,
        ClientApplicationConfig {
            current_instance: instance,
            world_catalog: worlds,
            world_manifests: manifests,
            bootstrap_terrain: Some(terrain.clone()),
            tool_profile: ToolProfile::new(
                [
                    ToolDefinition::new(ToolKind::Shovel, 1.0, 0.25, TerrainMaterialSet::SOIL),
                    ToolDefinition::new(ToolKind::Pickaxe, 1.0, 0.25, TerrainMaterialSet::ROCK),
                ],
                500,
            ),
            require_earth: false,
        },
        Some(session),
        now,
    )?;

    peer.try_send_server(ServerMessage::Welcome {
        server_tick: 1,
        id: minimium_core::ids::PlayerId::new(1),
        map_fingerprint: map.fingerprint(),
        health: 100,
        position: map.spawn(),
        position_mm: Position3Mm([0, 2000, 0]),
        context,
        manifest,
        transition_id: None,
        clock_sample: clock_sample(1)?,
        environment: environment(),
    })?;
    let mut time_index = 1;
    core.step(
        ClientStepInput {
            now: time_at(base, time_index),
            prediction: None,
        },
        |_| Ok(None),
        None,
    )?;
    let (_handle, request) = content_peer
        .take_request()
        .ok_or("Welcome did not request content")?;
    let ContentJobRequest::World(request) = request else {
        return Err("Welcome unexpectedly requested materials".into());
    };
    let resolved = ResolvedWorldContent::prepare(
        request.ticket,
        manifest,
        map.clone(),
        None,
        terrain_profile,
        None,
    )?;
    content_peer.complete(
        ContentJobTicket::World(request.ticket),
        ContentJobResult::World(Ok(PreparedWorldContent::prepare(resolved)?)),
    )?;
    time_index += 1;
    core.step(
        ClientStepInput {
            now: time_at(base, time_index),
            prediction: None,
        },
        |_| Ok(None),
        None,
    )?;
    if core.view().context != Some(context) {
        return Err("session content completion did not activate the welcomed world".into());
    }

    peer.try_send_server(ServerMessage::ImpactSync {
        context,
        complete: false,
    })?;
    peer.try_send_server(ServerMessage::ImpactSync {
        context,
        complete: true,
    })?;
    peer.try_send_server(ServerMessage::Excavation {
        context,
        message: minimium_protocol::ExcavationMessage::BatchBegin {
            sync_id: 1,
            base_revision: 0,
            revision: 0,
            chunks: 0,
            craters: 0,
            planet_radius_m: 1.0,
            full: true,
        },
    })?;
    peer.try_send_server(ServerMessage::Excavation {
        context,
        message: minimium_protocol::ExcavationMessage::BatchEnd {
            sync_id: 1,
            revision: 0,
        },
    })?;
    time_index += 1;
    core.step(
        ClientStepInput {
            now: time_at(base, time_index),
            prediction: None,
        },
        |_| Ok(None),
        None,
    )?;
    if !core
        .session()
        .is_some_and(|session| session.impact_ready() && session.excavation_ready())
    {
        return Err(
            "initial impact/excavation synchronization did not open CPU movement readiness".into(),
        );
    }

    let mut readiness = WorldReadiness::new()?;
    let mut latest_motion = None;
    for frame in 0..12 {
        time_index += 17;
        if frame % 3 == 0 {
            peer.try_send_server(ServerMessage::EnvironmentSync {
                context,
                clock_sample: clock_sample(2 + frame / 3)?,
                environment: environment(),
            })?;
        }
        let output = core.step(
            ClientStepInput {
                now: time_at(base, time_index),
                prediction: Some(minimium_client_runtime::PredictionInput {
                    direction: [1.0, 0.0],
                    ..Default::default()
                }),
            },
            |request| {
                let map = request
                    .world_catalog
                    .get(&request.instance)
                    .ok_or("missing map")?;
                let owner = request.context.map_or(
                    WorldReadinessOwner::Bootstrap(request.instance),
                    WorldReadinessOwner::Active,
                );
                prepare_collision(
                    &mut readiness,
                    owner,
                    request.manifest,
                    request.previous_position,
                    request.position,
                    map,
                    request.terrain_inputs,
                )
            },
            None,
        )?;
        if output.movement_queue_error.is_some() {
            return Err("CPU movement input was not accepted by the session".into());
        }
        while let Some(message) = peer.receive_client() {
            if let ClientMessage::WorldInput {
                input: ClientWorldInput::Motion(sample),
                ..
            } = message
            {
                latest_motion = Some(sample);
            }
        }
    }
    let predicted = core.view().player_position_m;
    if predicted[0].abs() < 1.0e-4 {
        return Err("CPU prediction did not produce nonzero horizontal movement".into());
    }
    let motion = latest_motion.ok_or("no typed motion sample reached the local peer")?;
    peer.try_send_server(ServerMessage::Snapshot {
        server_time_ms: motion.target_tick.max(5) * 50,
        server_tick: motion.target_tick.max(5),
        applied_input: (motion.input_seq, motion.target_tick.max(5)),
        resolved_input_seq: motion.input_seq,
        context,
        full_snapshot: true,
        self_movement: motion.intent,
        self_position: map.spawn(),
        self_position_mm: Position3Mm([0, 2000, 0]),
        self_velocity: [0.0; 3],
        self_grounded: false,
        players: Box::new(RemotePlayers::default()),
        removed_players: RemotePlayerIds::default(),
    })?;
    time_index += 17;
    let reconciled = core.step(
        ClientStepInput {
            now: time_at(base, time_index),
            prediction: None,
        },
        |_| Ok(None),
        None,
    )?;
    if !reconciled.session_events.iter().any(|applied| {
        matches!(
            applied.event,
            minimium_client_runtime::SessionEvent::Message(ServerMessage::Snapshot { .. })
        )
    }) {
        return Err("server snapshot was not dispatched through the application step".into());
    }
    let current_view = core.view();
    if current_view.context != Some(context) || current_view.character.position[0].abs() > 0.01 {
        return Err("read-only view did not reflect snapshot reconciliation".into());
    }
    println!(
        "predicted {predicted:?}; reconciled view {:?}",
        current_view.player_position_m
    );

    // The injected peer can send the same correlated receipt as the live server.
    let patch = minimium_protocol::MovementPatch::Speed(20.0);
    let submission = core.request_movement_policy(patch, None, time_at(base, time_index));
    if submission.receipt.stage != minimium_commands::results::Stage::Submitted {
        return Err("policy patch was not submitted".into());
    }
    let mut policy = patch
        .apply(minimium_simulation::MovementPolicy::default())
        .ok_or("invalid policy")?;
    policy.revision += 1;
    peer.try_send_server(ServerMessage::PolicyReply(minimium_protocol::PolicyReply {
        request_id: submission.identity.request_id,
        context,
        result: Ok(policy),
    }))?;
    time_index += 1;
    let acknowledged = core.step(
        ClientStepInput {
            now: time_at(base, time_index),
            prediction: None,
        },
        |_| Ok(None),
        None,
    )?;
    if !acknowledged.movement_results.iter().any(|r| {
        r.identity == submission.identity
            && r.receipt.stage == minimium_commands::results::Stage::OwnerCommitted
    }) {
        return Err("policy acknowledgement missing".into());
    }

    readiness.shutdown_until(Instant::now() + Duration::from_secs(2));
    let _ = core.finish_until(
        Instant::now() + Duration::from_secs(2),
        time_at(base, time_index + 1),
    );
    Ok(())
}

fn prepare_collision(
    readiness: &mut WorldReadiness,
    owner: WorldReadinessOwner,
    manifest: WorldManifest,
    previous: [f32; 3],
    candidate: [f32; 3],
    map: &WorldMap,
    terrain: minimium_procgen::TerrainInputs,
) -> Result<Option<CollisionSnapshot>, String> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if Instant::now() >= deadline {
            return Err("CPU collision preparation exceeded its bounded host wait".into());
        }
        if let Some(snapshot) = readiness
            .ensure_collision_for_step(owner, manifest, previous, candidate, map, terrain.clone())
            .map_err(|error| error.to_string())?
        {
            return Ok(Some(snapshot));
        }
        std::thread::yield_now();
    }
}

fn time_at(base: Instant, milliseconds: u64) -> minimium_client_runtime::ClientTime {
    minimium_client_runtime::ClientTime::new(base + Duration::from_millis(milliseconds))
}

fn clock_sample(tick: u64) -> Result<ClockSample, Box<dyn std::error::Error>> {
    Ok(ClockSample::new(
        ClockId::MVP,
        ClockRevision::INITIAL,
        SessionNonce::new([0x31; 16]),
        MonotonicMs::new(tick * 50),
        WorldTimeMs::new(1_800_000_000_000 + (tick * 50) as i64)?,
        ClockMode::RealTime,
        TimeScalePpm::ONE_X,
        OfflinePolicy::Pause,
    )?
    .with_boundary(minimium_environment::time::SimulationBoundary::new(
        tick,
        ClockRevision::INITIAL,
    )?))
}

fn environment() -> EnvironmentDescriptor {
    let identity = minimium_assets::embedded_sky_catalog_identity();
    EnvironmentDescriptor::new(
        ClockId::MVP,
        EnvironmentProfile::Interior,
        AstronomyModelRevision::CURRENT,
        CatalogueId::CURRENT,
        CatalogueSha256::new(identity.sha256),
    )
}
