use crate::parse_world_definition;
use minimium_core::world::{GenerationRevision, GenerationSpec, WorldSeed, WorldTopology};
use minimium_procgen::{generate_dungeon, generator_digest, DungeonSpec, DungeonTile};

fn definition_text() -> String {
    let generation =
        GenerationSpec::new(WorldSeed::new(99), GenerationRevision::new(1).unwrap(), 10).unwrap();
    let blueprint = generate_dungeon(generation, DungeonSpec::new(48, 40, 4, 5, 1).unwrap());
    let digest = generator_digest(WorldTopology::Dungeon, generation)
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let mut source = format!(
        "MINIMIUM_WORLD_DEFINITION 1\nmanifest 8 dungeon 99 1 10 1 {digest}\ndungeon 48 40\n"
    );
    for room in blueprint.rooms() {
        source.push_str(&format!(
            "room {} {} {} {} {} {} {}\n",
            room.origin_id.get(),
            room.x,
            room.y,
            room.width,
            room.height,
            room.center[0],
            room.center[1]
        ));
    }
    for y in 0..40 {
        for x in 0..48 {
            let tile = match blueprint.tile(x, y).unwrap() {
                DungeonTile::Void => "void",
                DungeonTile::Room => "room",
                DungeonTile::Corridor => "corridor",
            };
            source.push_str(&format!("tile {x} {y} {tile}\n"));
        }
    }
    source
}

#[test]
fn authored_dungeon_round_trips_through_the_shared_blueprint_validator() {
    let definition = parse_world_definition(&definition_text()).unwrap();
    assert_eq!(definition.manifest().topology(), WorldTopology::Dungeon);
    assert_eq!(definition.manifest().generation().seed().get(), 99);
    assert_eq!(definition.dungeon().dimensions(), (48, 40));
    assert_eq!(definition.dungeon().rooms().len(), 5);
    assert_ne!(
        definition.dungeon().spawn(),
        definition.dungeon().entrance()
    );
}

#[test]
fn authored_dungeon_rejects_incomplete_and_duplicate_tile_records() {
    let valid = definition_text();
    let incomplete = valid
        .lines()
        .filter(|line| !line.starts_with("tile 47 39 "))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(parse_world_definition(&incomplete).is_err());

    let duplicate = format!("{valid}tile 0 0 void\n");
    assert!(parse_world_definition(&duplicate).is_err());
}
