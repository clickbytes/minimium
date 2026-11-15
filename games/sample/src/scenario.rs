use minimium_procgen::{
    generate_dungeon, generator_digest, materialize_dungeon_map, DungeonSpec, GenerationError,
    PlanetPoint, PlanetReliefSamplingPolicy, PlanetSurfaceSpec, PlanetTerrain,
};
use minimium_world::world::{
    GenerationRevision, GenerationSpec, PlanetDefinition, PlanetFace, WorldContentRevision,
    WorldDefinitionId, WorldInstanceId, WorldManifest, WorldMap, WorldSeed, WorldTopology,
    PLANET_SAMPLE_COORDINATE_SCALE,
};
use std::sync::Arc;

const CONTENT_REVISION: u64 = 2;
const GENERATION_REVISION: u32 = 6;
const POSITION_QUANTUM_MM: u32 = 10;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DemoPortal {
    pub position: [f32; 3],
    pub target_instance: WorldInstanceId,
    pub destination: [f32; 3],
}

#[derive(Clone, Debug)]
pub struct GeneratedWorld {
    pub instance: WorldInstanceId,
    pub manifest: WorldManifest,
    pub map: WorldMap,
    pub resolved_relief_policy: Option<minimium_procgen::ResolvedPlanetReliefPolicy>,
    pub portal: Option<DemoPortal>,
}

#[derive(Clone, Copy)]
struct SamplePlanetLandmarks {
    spawn: PlanetPoint,
    entrance: PlanetPoint,
}

const PLANET_SPAWN_CLEARANCE_MM: i64 = 2_000;

/// Canonical sample material policy for any manifest accepted by the current
/// generator contract, bound to its resolved map identity.
pub fn terrain_material_profile(
    manifest: WorldManifest,
    map: &WorldMap,
) -> Result<minimium_procgen::ResolvedTerrainMaterialProfile, GenerationError> {
    minimium_procgen::validate_world_manifest(manifest)?;
    if manifest.source() == minimium_world::world::WorldSource::Authored
        && manifest.generator_digest()
            != minimium_world::world::GeneratorDigest::authored_map_fingerprint(map.fingerprint())
    {
        return Err(GenerationError::invalid_manifest(
            "authored terrain profile does not match the resolved map fingerprint",
        ));
    }
    Ok(default_terrain_material_profile().bind(manifest, map.fingerprint()))
}

/// The canonical sample policy selected by the sample application's content
/// composer before an authored map receives its runtime manifest.
pub fn default_terrain_material_profile() -> minimium_procgen::TerrainMaterialProfile {
    minimium_procgen::TerrainMaterialProfile::new(1_500)
        .expect("sample soil depth is within the supported range")
}

pub fn planet_relief_sampling_policy(
    manifest: WorldManifest,
) -> Result<Option<PlanetReliefSamplingPolicy>, GenerationError> {
    minimium_procgen::validate_world_manifest(manifest)?;
    if manifest.source() != minimium_world::world::WorldSource::Generated
        || manifest.topology() != WorldTopology::Planet
    {
        return Ok(None);
    }
    let planet = manifest.planet().ok_or_else(|| {
        GenerationError::invalid_manifest("generated planet has no planet definition")
    })?;
    let policy = match planet.mode {
        0 => PlanetReliefSamplingPolicy::cube_face_ramp(
            PlanetFace::PositiveY,
            PLANET_SAMPLE_COORDINATE_SCALE / 256,
            PLANET_SAMPLE_COORDINATE_SCALE / 64,
        )
        .ok_or_else(|| GenerationError::invalid_manifest("invalid sample relief policy"))?,
        1 => PlanetReliefSamplingPolicy::identity(),
        _ => {
            return Err(GenerationError::invalid_manifest(
                "unsupported sample planet mode",
            ));
        }
    };
    Ok(Some(policy))
}

pub fn resolved_planet_relief_policy(
    manifest: WorldManifest,
    map: &WorldMap,
) -> Result<Option<minimium_procgen::ResolvedPlanetReliefPolicy>, GenerationError> {
    Ok(planet_relief_sampling_policy(manifest)?.map(|policy| {
        minimium_procgen::ResolvedPlanetReliefPolicy::bind(policy, manifest, map.fingerprint())
    }))
}

/// Recreates one generated map from its complete manifest without inferring a
/// world role from its instance ID or transforming its recorded seed.
pub fn materialize_manifest_map(
    manifest: WorldManifest,
    geography: Option<Arc<dyn minimium_procgen::PreparedGeography>>,
) -> Result<WorldMap, GenerationError> {
    minimium_procgen::validate_world_manifest(manifest)?;
    match manifest.topology() {
        WorldTopology::Planet => {
            let definition = manifest.planet().ok_or_else(|| {
                GenerationError::invalid_manifest(
                    "generated planet manifest has no planet definition",
                )
            })?;
            let surface = PlanetSurfaceSpec::new(
                definition.radius_mm,
                u64::from(definition.mountain_limit_mm),
            )
            .ok_or_else(|| {
                GenerationError::invalid_manifest("invalid planet surface in manifest")
            })?;
            let relief_policy = planet_relief_sampling_policy(manifest)?.ok_or_else(|| {
                GenerationError::invalid_manifest("generated planet has no relief policy")
            })?;
            let terrain = PlanetTerrain::new_with_relief_policy(
                manifest.generation(),
                surface,
                Some(definition),
                geography.into(),
                relief_policy,
            )?;
            let landmarks = sample_planet_landmarks(&terrain)?;
            materialize_planet_map(landmarks.spawn, landmarks.entrance)
        }
        WorldTopology::Dungeon => {
            if manifest.planet().is_some() {
                return Err(GenerationError::invalid_manifest(
                    "generated dungeon manifest contains planet settings",
                ));
            }
            let spec = DungeonSpec::new(48, 40, 4, 5, 1).ok_or_else(|| {
                GenerationError::invalid_blueprint("invalid sample dungeon settings")
            })?;
            let blueprint = generate_dungeon(manifest.generation(), spec);
            materialize_dungeon_map(&blueprint)
        }
    }
}

/// Generates the default seeded planet and connected dungeon.
pub fn generate_demo_worlds() -> Result<[GeneratedWorld; 2], GenerationError> {
    generate_worlds(crate::DEFAULT_SEED)
}

/// A server seed identifies both the planet and its connected dungeon.
pub fn generate_worlds(seed: u128) -> Result<[GeneratedWorld; 2], GenerationError> {
    let definition = crate::ResolvedPlanetConfig::procedural(seed).definition();
    generate_worlds_with_planet(seed, definition)
}

pub fn generate_worlds_with_planet(
    seed: u128,
    definition: PlanetDefinition,
) -> Result<[GeneratedWorld; 2], GenerationError> {
    generate_worlds_with_planet_and_geography(seed, definition, None)
}

pub fn generate_worlds_with_planet_and_geography(
    seed: u128,
    definition: PlanetDefinition,
    geography: Option<Arc<dyn minimium_procgen::PreparedGeography>>,
) -> Result<[GeneratedWorld; 2], GenerationError> {
    if !definition.is_valid() {
        return Err(GenerationError::invalid_manifest(
            "invalid planet definition",
        ));
    }
    let surface = PlanetSurfaceSpec::new(
        definition.radius_mm,
        u64::from(definition.mountain_limit_mm),
    )
    .ok_or_else(|| GenerationError::invalid_manifest("invalid planet surface"))?;
    generate_worlds_with(seed, surface, Some(definition), geography)
}

fn generate_worlds_with(
    seed: u128,
    planet_surface: PlanetSurfaceSpec,
    planet_definition: Option<PlanetDefinition>,
    geography: Option<Arc<dyn minimium_procgen::PreparedGeography>>,
) -> Result<[GeneratedWorld; 2], GenerationError> {
    let planet_generation = generation(seed)?;
    let planet_manifest = manifest(
        1,
        WorldTopology::Planet,
        planet_generation,
        planet_definition,
    );
    let relief_policy = planet_relief_sampling_policy(planet_manifest)?
        .ok_or_else(|| GenerationError::invalid_manifest("sample planet has no relief policy"))?;
    let terrain = PlanetTerrain::new_with_relief_policy(
        planet_generation,
        planet_surface,
        planet_definition,
        geography.into(),
        relief_policy,
    )?;
    let landmarks = sample_planet_landmarks(&terrain)?;
    let planet_map = materialize_planet_map(landmarks.spawn, landmarks.entrance)?;
    let resolved_relief_policy = resolved_planet_relief_policy(planet_manifest, &planet_map)?
        .ok_or_else(|| GenerationError::invalid_manifest("sample planet policy was not bound"))?;

    let dungeon_generation = generation(seed ^ 0x0030_0000_0000_0003)?;
    let dungeon_spec = DungeonSpec::new(48, 40, 4, 5, 1)
        .ok_or_else(|| GenerationError::invalid_blueprint("demo dungeon settings are invalid"))?;
    let blueprint = generate_dungeon(dungeon_generation, dungeon_spec);
    let dungeon_map = materialize_dungeon_map(&blueprint)?;

    let planet_instance = id(1);
    let dungeon_instance = id(2);
    let planet_entrance = point_to_world(landmarks.entrance.position_mm.coordinates());
    let dungeon_entrance = tile_to_world(blueprint.entrance());
    let planet = GeneratedWorld {
        instance: planet_instance,
        manifest: planet_manifest,
        portal: Some(DemoPortal {
            position: planet_entrance,
            target_instance: dungeon_instance,
            destination: dungeon_map.spawn(),
        }),
        map: planet_map,
        resolved_relief_policy: Some(resolved_relief_policy),
    };
    let dungeon = GeneratedWorld {
        instance: dungeon_instance,
        manifest: manifest(
            2,
            WorldTopology::Dungeon,
            dungeon_generation,
            planet_definition,
        ),
        portal: Some(DemoPortal {
            position: dungeon_entrance,
            target_instance: planet_instance,
            destination: planet_entrance,
        }),
        map: dungeon_map,
        resolved_relief_policy: None,
    };
    Ok([planet, dungeon])
}

fn sample_planet_landmarks(
    terrain: &PlanetTerrain,
) -> Result<SamplePlanetLandmarks, GenerationError> {
    let scale = PLANET_SAMPLE_COORDINATE_SCALE;
    // PlanetTerrain::is_earth is exactly the old first `mode == 1` branch;
    // the later legacy `mode == 1` route branch was unreachable.
    let (u, start, step) = if terrain.is_earth() {
        let step = scale / 8192;
        (
            (i64::from(scale) * 566 / 1000) as i32,
            (i64::from(scale) * 527 / 10_000) as i32,
            step,
        )
    } else {
        (0, -(scale / 512), scale / 2048)
    };
    let mut samples = Vec::with_capacity(9);
    for index in 0..9 {
        let sample = PlanetFace::PositiveY
            .cube_sample(u, start + index as i32 * step)
            .expect("fixed sample planet route coordinates lie inside the face");
        samples.push(terrain.sample(sample)?);
    }
    let approach: [PlanetPoint; 9] = samples
        .try_into()
        .map_err(|_| GenerationError::materialization("invalid route sample count"))?;
    Ok(SamplePlanetLandmarks {
        spawn: approach[0],
        entrance: approach[8],
    })
}

fn materialize_planet_map(
    spawn_point: PlanetPoint,
    entrance_point: PlanetPoint,
) -> Result<WorldMap, GenerationError> {
    let spawn_mm = radial_clearance(
        spawn_point.position_mm.coordinates(),
        PLANET_SPAWN_CLEARANCE_MM,
    );
    let spawn = point_to_world(spawn_mm);
    let entrance = point_to_world(entrance_point.position_mm.coordinates());
    let marker = minimium_world::world::MapBox {
        center: [entrance[0], entrance[1] + 0.6, entrance[2]],
        size: [0.65, 1.2, 0.65],
        color: [0.86, 0.53, 0.16],
        solid: false,
    };
    WorldMap::from_parts(spawn, vec![marker], Vec::new())
        .map_err(GenerationError::materialization)
        .map(|map| map.with_spawn_position_mm(minimium_world::world::Position3Mm(spawn_mm)))
}

fn radial_clearance(position: [i64; 3], distance_mm: i64) -> [i64; 3] {
    let length_squared = position
        .into_iter()
        .map(|coordinate| (coordinate * coordinate) as u64)
        .sum::<u64>();
    let length = integer_sqrt(length_squared) as i64;
    position.map(|coordinate| {
        let numerator = coordinate * distance_mm;
        let offset = if numerator < 0 {
            -((-numerator + length / 2) / length)
        } else {
            (numerator + length / 2) / length
        };
        coordinate + offset
    })
}

fn integer_sqrt(value: u64) -> u64 {
    if value < 2 {
        return value;
    }
    let mut current = value;
    let mut next = current.div_ceil(2);
    while next < current {
        current = next;
        next = (current + value / current) / 2;
    }
    current
}

fn generation(seed: u128) -> Result<GenerationSpec, GenerationError> {
    GenerationSpec::new(
        WorldSeed::new(seed),
        GenerationRevision::new(GENERATION_REVISION)
            .expect("current generator revision is nonzero"),
        POSITION_QUANTUM_MM,
    )
    .ok_or_else(|| GenerationError::invalid_manifest("position quantum must be nonzero"))
}

fn manifest(
    definition: u128,
    topology: WorldTopology,
    generation: GenerationSpec,
    planet_definition: Option<PlanetDefinition>,
) -> WorldManifest {
    let manifest = WorldManifest::new(
        WorldDefinitionId::new(definition).expect("static demo definition is nonzero"),
        topology,
        generation,
        WorldContentRevision::new(CONTENT_REVISION)
            .expect("static demo content revision is nonzero"),
        generator_digest(topology, generation),
    );
    if topology == WorldTopology::Planet {
        if let Some(definition) = planet_definition {
            return manifest.with_planet(definition);
        }
        panic!("generated planet manifest requires its planet definition");
    }
    manifest
}

fn id(value: u128) -> WorldInstanceId {
    WorldInstanceId::new(value).expect("static demo instance is nonzero")
}

fn point_to_world(position: [i64; 3]) -> [f32; 3] {
    position.map(|value| value as f32 / 1000.0)
}

fn tile_to_world(tile: [u16; 2]) -> [f32; 3] {
    [f32::from(tile[0]) + 0.5, 0.0, f32::from(tile[1]) + 0.5]
}
