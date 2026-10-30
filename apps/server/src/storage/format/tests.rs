use super::*;
use crate::world_registry::WorldRegistry;
use minimium_core::{
    identity::PlayerProfileId,
    world::{CharacterGravity, Position3Mm, WorldOrientationQ30},
};
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

fn temporary_directory(label: &str) -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "minimium-epic003-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&directory).unwrap();
    directory
}

#[test]
fn v6_save_restores_instance_position_and_portal_return_location() {
    let generated = minimium_procgen::generate_demo_worlds().unwrap();
    let primary = generated[0].instance;
    let dungeon = generated[1].instance;
    let mut registry = WorldRegistry::from_generated_worlds(generated.to_vec());
    let mut store = PlayerStore::default();
    let profile = PlayerProfileId::from_bytes([0x35; 16]);
    let mut next_id = 1;
    let claimed = store
        .claim(
            profile,
            &mut next_id,
            generated[0].map.spawn(),
            None,
            primary,
            CharacterGravity::Radial { center: [0.0; 3] },
        )
        .unwrap();
    let position = [14.25, 0.0, 6.5];
    registry
        .join_at(dungeon, claimed.id, "Traveler".into(), position, 73)
        .unwrap();
    let return_location = WorldLocation::new(
        primary,
        Position3Mm::new(0, 99_240, 12_400),
        WorldOrientationQ30::IDENTITY,
    );
    store.set_return_location(claimed.id, return_location);

    let worlds = registry
        .iter()
        .map(|(instance, owner)| (instance, (*owner.map).clone(), owner.manifest))
        .collect::<Vec<_>>();
    let snapshot = store
        .save_snapshot(&registry, generated[0].map.fingerprint())
        .unwrap();
    let directory = temporary_directory("v6");
    snapshot.write(&directory).unwrap();

    let mut restored = load(&directory, &worlds).unwrap();
    let mut next_id = restored.next_player_id().unwrap();
    let player = restored
        .claim(
            profile,
            &mut next_id,
            [0.0; 3],
            None,
            primary,
            CharacterGravity::Radial { center: [0.0; 3] },
        )
        .unwrap();
    assert_eq!(player.id, claimed.id);
    assert_eq!(player.location.instance(), dungeon);
    assert_eq!(player.location.position().coordinates(), [14_250, 0, 6_500]);
    assert_eq!(player.return_location, return_location);
    assert_eq!(player.health, 73);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn v6_roundtrip_keeps_a_live_planet_position_clear_after_quantization() {
    let generated = minimium_procgen::generate_demo_worlds().unwrap();
    let primary = generated[0].instance;
    let gravity = CharacterGravity::Radial { center: [0.0; 3] };
    let position = [0.0, 99.43038, 10.75904];
    let mut registry = WorldRegistry::from_generated_worlds(generated.to_vec());
    let mut store = PlayerStore::default();
    let profile = PlayerProfileId::from_bytes([0xa6; 16]);
    let mut next_id = 1;
    let claimed = store
        .claim(
            profile,
            &mut next_id,
            generated[0].map.spawn(),
            None,
            primary,
            gravity,
        )
        .unwrap();
    registry
        .join_at(primary, claimed.id, "Traveler".into(), position, 73)
        .unwrap();
    let live_player = registry
        .for_player(claimed.id)
        .unwrap()
        .world
        .player(claimed.id)
        .unwrap();
    store
        .update_from_player(live_player, primary, gravity)
        .unwrap();
    store.set_return_location(
        claimed.id,
        crate::world_location::world_location(primary, position, gravity),
    );

    let worlds = registry
        .iter()
        .map(|(instance, owner)| (instance, (*owner.map).clone(), owner.manifest))
        .collect::<Vec<_>>();
    let snapshot = store
        .save_snapshot(&registry, generated[0].map.fingerprint())
        .unwrap();
    let directory = temporary_directory("radial-contact");
    snapshot.write(&directory).unwrap();

    let mut restored = load(&directory, &worlds).unwrap();
    let mut next_id = restored.next_player_id().unwrap();
    let player = restored
        .claim(profile, &mut next_id, [0.0; 3], None, primary, gravity)
        .unwrap();
    let restored_position = player
        .location
        .position()
        .coordinates()
        .map(|value| value as f32 / 1000.0);
    assert!(generated[0]
        .map
        .can_place_player_with_gravity(restored_position, gravity));
    assert_eq!(
        crate::world_location::world_location(primary, position, gravity).position(),
        player.location.position()
    );
    assert_eq!(player.health, 73);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn v2_and_v3_snapshots_migrate_only_to_the_matching_legacy_world() {
    let map = minimium_core::world::WorldMap::from_parts(
        [0.0; 3],
        vec![minimium_core::world::MapBox {
            center: [4.0, 4.0, 4.0],
            size: [1.0; 3],
            color: [0.5; 3],
            solid: false,
        }],
        Vec::new(),
    )
    .unwrap();
    let registry = WorldRegistry::legacy_primary(map.clone());
    let (instance, owner) = registry.iter().next().unwrap();
    let worlds = vec![(instance, map.clone(), owner.manifest)];
    let profile = PlayerProfileId::from_bytes([0x71; 16]);

    for version in [2, 3] {
        let directory = temporary_directory("legacy");
        let health = if version == 2 { "" } else { "\t73" };
        let snapshot = format!(
            "MINIMIUM_WORLD\t{version}\t{:016x}\n17\t{}\tLegacy\t1.25\t2.5\t3.75{health}\n",
            map.fingerprint(),
            profile.to_hex()
        );
        fs::write(directory.join("world.snapshot"), snapshot).unwrap();
        let mut restored = load(&directory, &worlds).unwrap();
        let mut next_id = restored.next_player_id().unwrap();
        let player = restored
            .claim(
                profile,
                &mut next_id,
                [0.0; 3],
                None,
                instance,
                CharacterGravity::FixedY,
            )
            .unwrap();
        assert_eq!(player.location.instance(), instance);
        assert_eq!(
            player.location.position().coordinates(),
            [1_250, 2_500, 3_750]
        );
        assert_eq!(player.return_location, player.location);
        assert_eq!(
            player.health,
            if version == 2 { PLAYER_MAX_HEALTH } else { 73 }
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
