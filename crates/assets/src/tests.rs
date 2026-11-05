use crate::parse_world_definition;
use minimium_world::world::{DungeonTile, WorldTopology};

fn definition_text() -> String {
    let digest = "00".repeat(32);
    let mut source = format!(
        "MINIMIUM_WORLD_DEFINITION 1\nmanifest 8 dungeon 99 1 10 1 {digest}\ndungeon 48 40\n"
    );
    for x in [2, 10, 18, 26, 34] {
        source.push_str(&format!("room {} {x} 2 3 3 {} 3\n", x + 1, x + 1));
    }
    for y in 0..40 {
        for x in 0..48 {
            let room = [2, 10, 18, 26, 34]
                .into_iter()
                .any(|origin| (origin..origin + 3).contains(&x) && (2..5).contains(&y));
            let corridor = y == 3 && (5..=36).contains(&x);
            let tile = match if room {
                DungeonTile::Room
            } else if corridor {
                DungeonTile::Corridor
            } else {
                DungeonTile::Void
            } {
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
