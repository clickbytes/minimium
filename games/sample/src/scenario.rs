use minimium_procgen::{
    generate_dungeon, generator_digest, materialize_dungeon_map, materialize_legacy_planet_map,
    materialize_planet_map, DungeonSpec, PlanetSurfaceSpec, PlanetTerrain,
};
use minimium_world::world::{
    GenerationRevision, GenerationSpec, PlanetDefinition, WorldContentRevision, WorldDefinitionId,
    WorldInstanceId, WorldManifest, WorldMap, WorldSeed, WorldTopology,
};
use std::sync::Arc;

const CONTENT_REVISION: u64 = 1;
const GENERATION_REVISION: u32 = 2;
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

/// Generates the default seeded planet and connected dungeon.
pub fn generate_demo_worlds() -> Result<[GeneratedWorld; 2], String> {
    generate_worlds(crate::DEFAULT_SEED)
}

/// A server seed identifies both the planet and its connected dungeon.
pub fn generate_worlds(seed: u128) -> Result<[GeneratedWorld; 2], String> {
    generate_worlds_with(
        seed,
        GENERATION_REVISION,
        minimium_procgen::planet_surface(),
        None,
        None,
    )
}

pub fn generate_worlds_with_planet(
    seed: u128,
    definition: PlanetDefinition,
) -> Result<[GeneratedWorld; 2], String> {
    generate_worlds_with_planet_revision(seed, definition, 3)
}

pub fn generate_worlds_with_planet_revision(
    seed: u128,
    definition: PlanetDefinition,
    revision: u32,
) -> Result<[GeneratedWorld; 2], String> {
    generate_worlds_with_planet_revision_and_geography(seed, definition, revision, None)
}

pub fn generate_worlds_with_planet_revision_and_geography(
    seed: u128,
    definition: PlanetDefinition,
    revision: u32,
    geography: Option<Arc<dyn minimium_procgen::PreparedGeography>>,
) -> Result<[GeneratedWorld; 2], String> {
    if !matches!(revision, 3 | 4) {
        return Err("unsupported planet generator revision".into());
    }
    let surface = PlanetSurfaceSpec::new(
        definition.radius_mm,
        u64::from(definition.mountain_limit_mm),
    )
    .ok_or("invalid planet surface")?;
    generate_worlds_with(seed, revision, surface, Some(definition), geography)
}

fn generate_worlds_with(
    seed: u128,
    revision: u32,
    planet_surface: PlanetSurfaceSpec,
    planet_definition: Option<PlanetDefinition>,
    geography: Option<Arc<dyn minimium_procgen::PreparedGeography>>,
) -> Result<[GeneratedWorld; 2], String> {
    let planet_generation = generation(seed, revision)?;
    let mut terrain = PlanetTerrain::new(planet_generation, planet_surface)
        .ok_or_else(|| String::from("demo planet generation settings are invalid"))?;
    if let Some(geography) = geography {
        terrain = terrain.with_geography(geography);
    }
    if let Some(definition) = planet_definition {
        terrain = terrain.with_definition(definition);
    }
    let landmarks = terrain.landmarks();
    let planet_map = materialize_planet_map(terrain, landmarks)
        .map_err(|error| format!("demo planet map: {error}"))?;

    let dungeon_generation = generation(seed ^ 0x0030_0000_0000_0003, revision)?;
    let dungeon_spec = DungeonSpec::new(48, 40, 4, 5, 1)
        .ok_or_else(|| String::from("demo dungeon settings are invalid"))?;
    let blueprint = generate_dungeon(dungeon_generation, dungeon_spec);
    let dungeon_map = materialize_dungeon_map(&blueprint)
        .map_err(|error| format!("demo dungeon map: {error}"))?;

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

/// Frozen v1 layout, used only to recognize and migrate existing demo saves.
pub fn generate_legacy_demo_worlds() -> Result<[GeneratedWorld; 2], String> {
    let legacy_generation = |seed| {
        GenerationSpec::new(
            WorldSeed::new(seed),
            GenerationRevision::new(1).expect("legacy revision is nonzero"),
            POSITION_QUANTUM_MM,
        )
        .expect("legacy quantum is valid")
    };
    let planet_generation = legacy_generation(0x0030_0000_0000_0001);
    let terrain = PlanetTerrain::new(
        planet_generation,
        PlanetSurfaceSpec::new(100_000, 300).expect("legacy surface is valid"),
    )
    .expect("legacy terrain is valid");
    let landmarks = terrain.landmarks();
    let planet_map = materialize_legacy_planet_map(terrain, landmarks)?;
    let dungeon_generation = legacy_generation(0x0030_0000_0000_0002);
    let blueprint = generate_dungeon(
        dungeon_generation,
        DungeonSpec::new(48, 40, 4, 5, 1).expect("legacy dungeon spec is valid"),
    );
    let dungeon_map = materialize_dungeon_map(&blueprint)?;
    let entrance = point_to_world(landmarks.entrance.position_mm.coordinates());
    let old_manifest = |definition, topology, generation| {
        WorldManifest::new(
            WorldDefinitionId::new(definition).expect("legacy definition is nonzero"),
            topology,
            generation,
            WorldContentRevision::new(1).expect("legacy content is nonzero"),
            generator_digest(topology, generation),
        )
    };
    Ok([
        GeneratedWorld {
            instance: id(1),
            manifest: old_manifest(1, WorldTopology::Planet, planet_generation),
            portal: None,
            map: planet_map,
        },
        GeneratedWorld {
            instance: id(2),
            manifest: old_manifest(2, WorldTopology::Dungeon, dungeon_generation),
            portal: Some(DemoPortal {
                position: tile_to_world(blueprint.entrance()),
                target_instance: id(1),
                destination: entrance,
            }),
            map: dungeon_map,
        },
    ])
}

fn generation(seed: u128, revision: u32) -> Result<GenerationSpec, String> {
    GenerationSpec::new(
        WorldSeed::new(seed),
        GenerationRevision::new(revision)
            .ok_or_else(|| String::from("generation revision must be nonzero"))?,
        POSITION_QUANTUM_MM,
    )
    .ok_or_else(|| String::from("position quantum must be nonzero"))
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
        WorldContentRevision::new(if generation.revision().get() >= 3 {
            2
        } else {
            CONTENT_REVISION
        })
        .expect("static demo content revision is nonzero"),
        generator_digest(topology, generation),
    );
    if let Some(definition) = planet_definition {
        manifest.with_planet(definition)
    } else {
        manifest
    }
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
