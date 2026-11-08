use minimium_procgen::{
    generate_dungeon, generator_digest, materialize_dungeon_map, materialize_planet_map,
    DungeonSpec, GenerationError, PlanetSurfaceSpec, PlanetTerrain,
};
use minimium_world::world::{
    GenerationRevision, GenerationSpec, PlanetDefinition, WorldContentRevision, WorldDefinitionId,
    WorldInstanceId, WorldManifest, WorldMap, WorldSeed, WorldTopology,
};
use std::sync::Arc;

const CONTENT_REVISION: u64 = 2;
const GENERATION_REVISION: u32 = 5;
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
    pub portal: Option<DemoPortal>,
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
            let mut terrain = PlanetTerrain::new(manifest.generation(), surface)
                .ok_or_else(|| {
                    GenerationError::invalid_manifest(
                        "invalid planet generation settings in manifest",
                    )
                })?
                .with_definition(definition);
            if let Some(geography) = geography {
                terrain = terrain.with_geography(geography);
            }
            let landmarks = terrain.landmarks();
            materialize_planet_map(terrain, landmarks)
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
    let mut terrain = PlanetTerrain::new(planet_generation, planet_surface).ok_or_else(|| {
        GenerationError::invalid_manifest("demo planet generation settings are invalid")
    })?;
    if let Some(geography) = geography {
        terrain = terrain.with_geography(geography);
    }
    if let Some(definition) = planet_definition {
        terrain = terrain.with_definition(definition);
    }
    let landmarks = terrain.landmarks();
    let planet_map = materialize_planet_map(terrain, landmarks)?;

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
        manifest: manifest(
            1,
            WorldTopology::Planet,
            planet_generation,
            planet_definition,
        ),
        portal: Some(DemoPortal {
            position: planet_entrance,
            target_instance: dungeon_instance,
            destination: dungeon_map.spawn(),
        }),
        map: planet_map,
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
    };
    Ok([planet, dungeon])
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
