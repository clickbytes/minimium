//! Public CPU-only geometry -> collision -> prediction consumer. Compile only.
use minimium_client_runtime::{
    world_readiness::{CollisionSnapshot, WorldReadiness, WorldReadinessOwner},
    ClientPrediction,
};
use minimium_procgen::{
    PreparedTerrainSource, TerrainInputs, TerrainMaterialProfile, TerrainOverlaySnapshot,
};
use minimium_simulation::{
    CharacterState, CollisionQuery, MovementPolicy, SIMULATION_STEP_SECONDS,
};
use minimium_world::world::{
    GenerationRevision, GenerationSpec, GeneratorDigest, MapBox, WorldContentRevision,
    WorldDefinitionId, WorldInstanceId, WorldManifest, WorldMap, WorldSeed, WorldTopology,
};
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let world = WorldInstanceId::new(1).unwrap();
    let map = WorldMap::from_parts(
        [0.0, 3.0, 0.0],
        vec![MapBox {
            center: [0.0; 3],
            size: [8.0, 1.0, 8.0],
            color: [0.3; 3],
            solid: true,
        }],
        Vec::new(),
    )?;
    let manifest = WorldManifest::new(
        WorldDefinitionId::new(1).unwrap(),
        WorldTopology::Dungeon,
        GenerationSpec::new(WorldSeed::new(7), GenerationRevision::new(6).unwrap(), 1).unwrap(),
        WorldContentRevision::new(1).unwrap(),
        GeneratorDigest::authored_map_fingerprint(map.fingerprint()),
    );
    let profile = TerrainMaterialProfile::new(1500)
        .unwrap()
        .bind(manifest, map.fingerprint());
    let source = PreparedTerrainSource::new(world, manifest, &map, None, profile, None)?;
    let terrain = TerrainInputs::new(source, TerrainOverlaySnapshot::empty_for(world))?;
    let owner = WorldReadinessOwner::Bootstrap(world);
    let mut provider = WorldReadiness::new()?;
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut snapshot = prepare(
        &mut provider,
        owner,
        manifest,
        map.spawn(),
        map.spawn(),
        &map,
        &terrain,
        deadline,
    )?;
    if !snapshot.matches(owner, manifest, &map, &terrain) || !snapshot.covers_position(map.spawn())
    {
        return Err("wrong CPU readiness identity or coverage".into());
    }
    let mut prediction = ClientPrediction::new(CharacterState::at_spawn(map.spawn()), None);
    let mut queries = CollisionQuery::default();
    prediction.accumulate_time(SIMULATION_STEP_SECONDS);
    loop {
        let step = prediction
            .predict_next(
                snapshot.collision(),
                &mut queries,
                [1.0, 0.0],
                0.0,
                MovementPolicy::default(),
                false,
                true,
                Some(manifest),
                None,
            )
            .ok_or("fixed step was not available")?;
        // A host never commits a speculative move outside its CPU coverage.
        let candidate = prediction.character.position;
        if snapshot.covers_position(candidate) {
            break;
        }
        prediction.character = step.previous_state;
        prediction.position_mm = step.previous_mm;
        prediction.accumulate_time(SIMULATION_STEP_SECONDS);
        snapshot = prepare(
            &mut provider,
            owner,
            manifest,
            prediction.character.position,
            candidate,
            &map,
            &terrain,
            deadline,
        )?;
    }
    provider.release_interest(owner);
    let _unfinished = provider.shutdown_until(Instant::now() + Duration::from_secs(2));
    Ok(())
}

// Physical host budget for asynchronous CPU work, separate from logical time.
fn prepare(
    provider: &mut WorldReadiness,
    owner: WorldReadinessOwner,
    manifest: WorldManifest,
    previous_position: [f32; 3],
    position: [f32; 3],
    map: &WorldMap,
    terrain: &TerrainInputs,
    deadline: Instant,
) -> Result<CollisionSnapshot, Box<dyn std::error::Error>> {
    loop {
        if Instant::now() >= deadline {
            return Err("CPU preparation remained pending".into());
        }
        if let Some(snapshot) = provider.ensure_collision_for_step(
            owner,
            manifest,
            previous_position,
            position,
            map,
            terrain.clone(),
        )? {
            return Ok(snapshot);
        }
        std::thread::yield_now();
    }
}
