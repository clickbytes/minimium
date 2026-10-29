use crate::{
    generate_dungeon, materialize_dungeon_map, materialize_planet_map, DungeonSpec,
    PlanetSurfaceSpec, PlanetTerrain,
};
use minimium_core::world::{
    GenerationRevision, GenerationSpec, GeneratorDigest, WorldContentRevision, WorldDefinitionId,
    WorldInstanceId, WorldManifest, WorldMap, WorldSeed, WorldTopology,
};

const CONTENT_REVISION: u64 = 1;
const GENERATION_REVISION: u32 = 1;
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

/// Generates the small deterministic planet and connected dungeon used by the
/// local end-to-end demonstration. The production generators remain stateless.
pub fn generate_demo_worlds() -> Result<[GeneratedWorld; 2], String> {
    let planet_generation = generation(0x0030_0000_0000_0001)?;
    let planet_surface = PlanetSurfaceSpec::new(100_000, 300)
        .ok_or_else(|| String::from("demo planet surface settings are invalid"))?;
    let terrain = PlanetTerrain::new(planet_generation, planet_surface)
        .ok_or_else(|| String::from("demo planet generation settings are invalid"))?;
    let landmarks = terrain.landmarks();
    let planet_map = materialize_planet_map(terrain, landmarks)
        .map_err(|error| format!("demo planet map: {error}"))?;

    let dungeon_generation = generation(0x0030_0000_0000_0002)?;
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
        manifest: manifest(1, WorldTopology::Planet, planet_generation),
        portal: Some(DemoPortal {
            position: planet_entrance,
            target_instance: dungeon_instance,
            destination: dungeon_map.spawn(),
        }),
        map: planet_map,
    };
    let dungeon = GeneratedWorld {
        instance: dungeon_instance,
        manifest: manifest(2, WorldTopology::Dungeon, dungeon_generation),
        portal: Some(DemoPortal {
            position: dungeon_entrance,
            target_instance: planet_instance,
            destination: planet_entrance,
        }),
        map: dungeon_map,
    };
    Ok([planet, dungeon])
}

fn generation(seed: u128) -> Result<GenerationSpec, String> {
    GenerationSpec::new(
        WorldSeed::new(seed),
        GenerationRevision::new(GENERATION_REVISION)
            .ok_or_else(|| String::from("generation revision must be nonzero"))?,
        POSITION_QUANTUM_MM,
    )
    .ok_or_else(|| String::from("position quantum must be nonzero"))
}

fn manifest(
    definition: u128,
    topology: WorldTopology,
    generation: GenerationSpec,
) -> WorldManifest {
    WorldManifest::new(
        WorldDefinitionId::new(definition).expect("static demo definition is nonzero"),
        topology,
        generation,
        WorldContentRevision::new(CONTENT_REVISION)
            .expect("static demo content revision is nonzero"),
        generator_digest(topology, generation),
    )
}

/// Digest of the code contract for one topology and generator revision.
pub fn generator_digest(topology: WorldTopology, generation: GenerationSpec) -> GeneratorDigest {
    let mut hash = 0xcbf29ce484222325_u64;
    let mut feed = |bytes: &[u8]| {
        for byte in bytes {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
        }
    };
    feed(b"minimium-procgen/philox4x32-10/v1");
    feed(&[match topology {
        WorldTopology::Planet => 0,
        WorldTopology::Dungeon => 1,
    }]);
    feed(&generation.revision().get().to_le_bytes());
    feed(&generation.position_quantum_mm().to_le_bytes());
    let mut bytes = [0_u8; 32];
    for (index, chunk) in bytes.as_chunks_mut::<8>().0.iter_mut().enumerate() {
        let value = hash.rotate_left((index * 11) as u32) ^ index as u64;
        chunk.copy_from_slice(&value.to_le_bytes());
    }
    GeneratorDigest::new(bytes)
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
