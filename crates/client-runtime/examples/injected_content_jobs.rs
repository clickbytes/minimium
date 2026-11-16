//! Compile-only real session/host exchange through bounded typed content jobs.
#[path = "support/local_content_jobs.rs"]
mod local_content_jobs;
#[path = "support/local_transport.rs"]
mod local_transport;

use local_content_jobs::LocalContentJobs;
use local_transport::LocalTransport;
use minimium_client_runtime::{
    ClientAuthentication, ClientSession, ContentJobPoll, ContentJobRequest, ContentJobResult,
    ContentJobTicket, ContentJobsPort, ContentJobsSession, PreparedWorldContent,
    ResolvedWorldContent, SessionState,
};
use minimium_environment::{
    time::{
        ClockId, ClockMode, ClockRevision, ClockSample, MonotonicMs, OfflinePolicy, SessionNonce,
        TimeScalePpm, WorldTimeMs,
    },
    AstronomyModelRevision, CatalogueId, CatalogueSha256, EnvironmentDescriptor,
    EnvironmentProfile,
};
use minimium_protocol::{ServerMessage, WorldContext};
use minimium_world::world::{
    GenerationRevision, GenerationSpec, GeneratorDigest, MapBox, MembershipEpoch, Position3Mm,
    WorldContentRevision, WorldDefinitionId, WorldInstanceId, WorldManifest, WorldMap, WorldSeed,
    WorldTopology,
};
use std::{num::NonZeroU64, sync::Arc};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let jobs = Arc::new(LocalContentJobs::default());
    let content_peer = jobs.peer();
    let transport = LocalTransport::default();
    let peer = transport.peer();
    let authentication = ClientAuthentication::new("local", b"bounded credential".to_vec())?;
    let service = ContentJobsSession::new(NonZeroU64::new(1).unwrap(), jobs.clone());
    let mut session = ClientSession::new(Box::new(transport), authentication, service)?;

    let map = Arc::new(WorldMap::from_parts(
        [0.0, 3.0, 0.0],
        vec![MapBox {
            center: [0.0; 3],
            size: [8.0, 1.0, 8.0],
            color: [0.3; 3],
            solid: true,
        }],
        Vec::new(),
    )?);
    let manifest = WorldManifest::new(
        WorldDefinitionId::new(1).unwrap(),
        WorldTopology::Dungeon,
        GenerationSpec::new(WorldSeed::new(7), GenerationRevision::new(6).unwrap(), 1).unwrap(),
        WorldContentRevision::new(1).unwrap(),
        GeneratorDigest::authored_map_fingerprint(map.fingerprint()),
    );
    let context = WorldContext::new(
        WorldInstanceId::new(1).unwrap(),
        manifest.content_revision(),
        MembershipEpoch::new(1).unwrap(),
    );
    let clock_sample = ClockSample::new(
        ClockId::MVP,
        ClockRevision::INITIAL,
        SessionNonce::new([0x42; 16]),
        MonotonicMs::new(0),
        WorldTimeMs::new(1_800_000_000_000)?,
        ClockMode::RealTime,
        TimeScalePpm::ONE_X,
        OfflinePolicy::Pause,
    )?;
    let identity = minimium_assets::embedded_sky_catalog_identity();
    let environment = EnvironmentDescriptor::new(
        ClockId::MVP,
        EnvironmentProfile::Interior,
        AstronomyModelRevision::CURRENT,
        CatalogueId::CURRENT,
        CatalogueSha256::new(identity.sha256),
    );
    peer.try_send_server(ServerMessage::Welcome {
        server_tick: 1,
        id: minimium_core::ids::PlayerId::new(1),
        map_fingerprint: map.fingerprint(),
        health: 100,
        position: map.spawn(),
        position_mm: Position3Mm([0, 3000, 0]),
        context,
        manifest,
        transition_id: None,
        clock_sample,
        environment,
    })?;
    let mut events = Vec::new();
    if session.poll(&mut events) != SessionState::AwaitingWelcome {
        return Err("session published content before host preparation".into());
    }

    let (handle, request) = content_peer
        .take_request()
        .ok_or("world request was not submitted")?;
    let ContentJobRequest::World(request) = request else {
        return Err("unexpected material job".into());
    };
    let ticket = ContentJobTicket::World(request.ticket);
    if handle.ticket() != ticket
        || request.ticket.context != context
        || request.manifest != manifest
    {
        return Err("request identity was not preserved".into());
    }
    if !matches!(jobs.poll(ticket), ContentJobPoll::Pending) {
        return Err("acceptance was confused with completion".into());
    }
    // Preparation runs in the host phase, never inside session submit/poll.
    let profile = minimium_procgen::TerrainMaterialProfile::new(1500)
        .unwrap()
        .bind(manifest, map.fingerprint());
    let content =
        ResolvedWorldContent::prepare(request.ticket, manifest, map, None, profile, None)?;
    let prepared = PreparedWorldContent::prepare(content)?;
    content_peer.complete(ticket, ContentJobResult::World(Ok(prepared)))?;

    if session.poll(&mut events) != SessionState::Connected
        || session.active_world_context() != Some(context)
    {
        return Err("session did not validate and install completed content".into());
    }
    let installed = session
        .resolved_world_content_for_context(context)
        .ok_or("installed content is unavailable")?;
    if installed.ticket() != request.ticket {
        return Err("installed content lost its completion ticket".into());
    }
    session.close();
    let _ = jobs.shutdown_until(std::time::Instant::now());
    Ok(())
}
