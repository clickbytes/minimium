use crate::composition::WorldDefinition;
use crate::world_registry::WorldRegistry;
use minimium_core::identity::PlayerProfileId;
use minimium_environment::time::{
    ClockId, ClockMode, ClockRevision, OfflinePolicy, SavedClockState, TimeScalePpm, WorldTimeMs,
};
use minimium_storage::{world_location, PlayerStore, StorageErrorKind};
use minimium_world::entity::ComponentRegistry;
use minimium_world::world::{CharacterGravity, Position3Mm, WorldLocation, WorldOrientationQ30};
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

fn registry_from_sample(worlds: Vec<minimium_game_sample::GeneratedWorld>) -> WorldRegistry {
    WorldRegistry::from_world_definitions(
        worlds
            .into_iter()
            .map(|world| WorldDefinition {
                instance: world.instance,
                manifest: world.manifest,
                map: world.map,
                initial_entities: Vec::new(),
            })
            .collect(),
    )
}

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

fn saved_clock() -> SavedClockState {
    let utc = WorldTimeMs::new(1_800_000_000_000).unwrap();
    SavedClockState::new(
        ClockId::MVP,
        ClockRevision::INITIAL,
        ClockMode::RealTime,
        utc,
        utc,
        TimeScalePpm::ONE_X,
        OfflinePolicy::Pause,
    )
    .unwrap()
}

#[test]
fn v10_save_restores_instance_position_and_portal_return_location() {
    let generated = minimium_game_sample::generate_demo_worlds().unwrap();
    let primary = generated[0].instance;
    let dungeon = generated[1].instance;
    let mut registry = registry_from_sample(generated.to_vec());
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
    let input = registry
        .snapshot_for_save(generated[0].map.fingerprint())
        .unwrap();
    let snapshot = store.save_snapshot(input, saved_clock()).unwrap();
    let directory = temporary_directory("v8");
    snapshot.write(&directory).unwrap();

    let mut restored = PlayerStore::load(&directory, &worlds, &ComponentRegistry::default())
        .unwrap()
        .player_store;
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
fn v10_roundtrip_keeps_a_live_planet_position_clear_after_quantization() {
    let generated = minimium_game_sample::generate_demo_worlds().unwrap();
    let primary = generated[0].instance;
    let gravity = CharacterGravity::Radial { center: [0.0; 3] };
    let position = [0.0, 99.43038, 10.75904];
    let mut registry = registry_from_sample(generated.to_vec());
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
        world_location::world_location(primary, position, gravity),
    );

    let worlds = registry
        .iter()
        .map(|(instance, owner)| (instance, (*owner.map).clone(), owner.manifest))
        .collect::<Vec<_>>();
    let input = registry
        .snapshot_for_save(generated[0].map.fingerprint())
        .unwrap();
    let snapshot = store.save_snapshot(input, saved_clock()).unwrap();
    let directory = temporary_directory("radial-contact");
    snapshot.write(&directory).unwrap();

    let mut restored = PlayerStore::load(&directory, &worlds, &ComponentRegistry::default())
        .unwrap()
        .player_store;
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
        world_location::world_location(primary, position, gravity).position(),
        player.location.position()
    );
    assert_eq!(player.health, 73);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn versions_two_through_nine_are_rejected_without_replacing_the_snapshot() {
    let map = minimium_world::world::WorldMap::from_parts(
        [0.0; 3],
        vec![minimium_world::world::MapBox {
            center: [4.0, 4.0, 4.0],
            size: [1.0; 3],
            color: [0.5; 3],
            solid: false,
        }],
        Vec::new(),
    )
    .unwrap();
    let registry = WorldRegistry::authored_primary(map.clone());
    let (instance, owner) = registry.iter().next().unwrap();
    let worlds = vec![(instance, map.clone(), owner.manifest)];
    for version in 2..=9 {
        let directory = temporary_directory("legacy");
        let snapshot = format!("MINIMIUM_WORLD\t{version}\t{:016x}\n", map.fingerprint());
        let path = directory.join("world.snapshot");
        fs::write(&path, &snapshot).unwrap();
        let error = match PlayerStore::load(&directory, &worlds, &ComponentRegistry::default()) {
            Ok(_) => panic!("unsupported snapshot version {version} was loaded"),
            Err(error) => error,
        };
        assert_eq!(error.kind(), StorageErrorKind::UnsupportedVersion);
        assert_eq!(fs::read(&path).unwrap(), snapshot.as_bytes());
        fs::remove_dir_all(directory).unwrap();
    }
}
