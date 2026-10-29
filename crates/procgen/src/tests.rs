use super::*;
use minimium_core::world::{
    CharacterGravity, GenerationRevision, GenerationSpec, PlanetEdge, PlanetFace, PlanetSectorKey,
    SectorKey, WorldSeed,
};
use std::collections::HashSet;

fn generation(seed: u128) -> GenerationSpec {
    GenerationSpec::new(
        WorldSeed::new(seed),
        GenerationRevision::new(1).unwrap(),
        10,
    )
    .unwrap()
}

#[test]
fn philox_matches_the_reference_zero_vector() {
    assert_eq!(
        philox4x32_10([0, 0], [0, 0, 0, 0]),
        [0x6627_e8d5, 0xe169_c58d, 0xbc57_ac4c, 0x9b00_dbd8]
    );
}

#[test]
fn addressed_samples_are_seekable_and_separated_by_channel() {
    let spec = generation(0x1234_5678_9abc_def0);
    let key = SectorKey::planet(PlanetFace::PositiveY, 17, 23).unwrap();
    let terrain = AddressedRandom::new(spec, GenerationChannel::Terrain, key);
    let objects = AddressedRandom::new(spec, GenerationChannel::Objects, key);
    let forward = [0, 1, 2, 3, 9, 24].map(|index| terrain.sample_u32(index));
    let reverse = [24, 9, 3, 2, 1, 0]
        .map(|index| terrain.sample_u32(index))
        .into_iter()
        .rev()
        .collect::<Vec<_>>();

    assert_eq!(forward.as_slice(), reverse);
    assert_ne!(forward[0], objects.sample_u32(0));
    assert_eq!(
        terrain.object_origin(5),
        AddressedRandom::new(spec, GenerationChannel::Terrain, key).object_origin(5)
    );
}

#[test]
fn every_cube_face_edge_has_matching_canonical_samples() {
    let edges = [
        (PlanetEdge::UNegative, 0_u16, 100_u16, 0_u32),
        (PlanetEdge::UPositive, 255, 100, 8),
        (PlanetEdge::VNegative, 100, 0, 1),
        (PlanetEdge::VPositive, 100, 255, 1),
    ];
    for face in PlanetFace::ALL {
        for (edge, u, v, local_edge) in edges {
            let key = PlanetSectorKey::new(face, u, v).unwrap();
            let neighbor = key.neighbor(edge);
            let reciprocal = [
                PlanetEdge::UNegative,
                PlanetEdge::UPositive,
                PlanetEdge::VNegative,
                PlanetEdge::VPositive,
            ]
            .into_iter()
            .find(|candidate| neighbor.neighbor(*candidate) == key)
            .expect("cube face edge has a reciprocal neighbor");
            let samples = |sector: PlanetSectorKey, edge: PlanetEdge| {
                (0..=8)
                    .map(|step| match edge {
                        PlanetEdge::UNegative => sector.cube_sample(0, step, 8),
                        PlanetEdge::UPositive => sector.cube_sample(8, step, 8),
                        PlanetEdge::VNegative => sector.cube_sample(step, 0, 8),
                        PlanetEdge::VPositive => sector.cube_sample(step, 8, 8),
                    })
                    .collect::<Option<Vec<_>>>()
                    .unwrap()
            };
            let own_edge = match edge {
                PlanetEdge::UNegative | PlanetEdge::UPositive => {
                    if local_edge == 0 {
                        PlanetEdge::UNegative
                    } else {
                        PlanetEdge::UPositive
                    }
                }
                PlanetEdge::VNegative | PlanetEdge::VPositive => {
                    if v == 0 {
                        PlanetEdge::VNegative
                    } else {
                        PlanetEdge::VPositive
                    }
                }
            };
            let own = samples(key, own_edge).into_iter().collect::<HashSet<_>>();
            let other = samples(neighbor, reciprocal)
                .into_iter()
                .collect::<HashSet<_>>();
            assert_eq!(own, other, "{face:?} {edge:?}");
        }
    }
}

#[test]
fn planet_route_is_quantized_traversable_and_has_bounded_height_steps() {
    let surface = PlanetSurfaceSpec::new(100_000, 300).unwrap();
    let terrain = PlanetTerrain::new(generation(42), surface).unwrap();
    let landmarks = terrain.landmarks();
    let map = materialize_planet_map(terrain, landmarks).unwrap();
    assert!(
        map.can_place_player_with_gravity(
            map.spawn(),
            CharacterGravity::Radial { center: [0.0; 3] },
        )
    );

    for point in landmarks.approach {
        assert!(point
            .position_mm
            .coordinates()
            .iter()
            .all(|value| *value % 10 == 0));
        assert!((99_700..=100_300).contains(&point.height_mm));
    }
    for pair in landmarks.approach.windows(2) {
        let height_step = pair[1].height_mm.abs_diff(pair[0].height_mm);
        assert!(
            height_step <= surface.relief_mm() / 2,
            "height step was {height_step}mm"
        );
    }
}

#[test]
fn dungeon_materialization_preserves_tiles_across_sector_boundaries() {
    let spec = DungeonSpec::new(48, 40, 4, 5, 1).unwrap();
    let blueprint = generate_dungeon(generation(77), spec);
    let repeated = generate_dungeon(generation(77), spec);
    assert_eq!(blueprint.dimensions(), (48, 40));
    assert!(blueprint.rooms().len() >= 3);
    assert_eq!(blueprint.spawn(), repeated.spawn());
    assert_eq!(blueprint.entrance(), repeated.entrance());

    for z in 0..40_u16 {
        for x in 0..48_u16 {
            let key = minimium_core::world::DungeonSectorKey::new(
                i32::from(x / 16),
                0,
                i32::from(z / 16),
            );
            let sector = blueprint.materialize_sector(key).unwrap();
            assert_eq!(
                sector.tile(x % 16, z % 16),
                blueprint.tile(x, z),
                "tile {x},{z}"
            );
            assert_eq!(blueprint.tile(x, z), repeated.tile(x, z));
        }
    }
}

#[test]
fn demo_worlds_are_distinct_and_portals_form_a_round_trip() {
    let [planet, dungeon] = generate_demo_worlds().unwrap();
    let outward = planet.portal.unwrap();
    let inward = dungeon.portal.unwrap();
    assert_ne!(planet.instance, dungeon.instance);
    assert_eq!(outward.target_instance, dungeon.instance);
    assert_eq!(inward.target_instance, planet.instance);
    assert_eq!(inward.destination, outward.position);
}
