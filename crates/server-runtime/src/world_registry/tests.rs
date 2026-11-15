use super::*;
use minimium_game_sample::generate_demo_worlds;
use minimium_procgen::{generate_dungeon, DungeonSpec, DungeonTile};
use minimium_protocol::WorldContext;
use minimium_simulation::MovementIntent;
use minimium_world::entity::{
    ComponentDescriptor, ComponentRegistry, ReplicationPolicy, SchemaVersion,
};
use minimium_world::world::{
    GenerationRevision, GenerationSpec, PlanetFace, SectorKey, SectorRevision, WorldSeed,
};
use std::collections::HashSet;

fn near_portal(position: [f32; 3], portal: [f32; 3]) -> bool {
    position
        .iter()
        .zip(portal)
        .map(|(player, target)| (*player - target).powi(2))
        .sum::<f32>()
        <= 9.0
}

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

#[test]
fn equal_coordinates_in_separate_instances_have_isolated_membership() {
    let worlds = generate_demo_worlds().unwrap();
    let planet = worlds[0].instance;
    let dungeon = worlds[1].instance;
    let mut registry = registry_from_sample(worlds.to_vec());
    let planet_player = PlayerId::new(10);
    let dungeon_player = PlayerId::new(11);
    let same_position = [12.0, 34.0, 56.0];

    registry
        .join_at(planet, planet_player, "Planet".into(), same_position, 100)
        .unwrap();
    registry
        .join_at(
            dungeon,
            dungeon_player,
            "Dungeon".into(),
            same_position,
            100,
        )
        .unwrap();

    assert_eq!(
        registry
            .for_player(planet_player)
            .unwrap()
            .world
            .player(planet_player)
            .unwrap()
            .position,
        same_position
    );
    assert_eq!(
        registry
            .for_player(dungeon_player)
            .unwrap()
            .world
            .player(dungeon_player)
            .unwrap()
            .position,
        same_position
    );
    assert!(registry
        .get(planet)
        .unwrap()
        .world
        .player(dungeon_player)
        .is_none());
    assert!(registry
        .get(dungeon)
        .unwrap()
        .world
        .player(planet_player)
        .is_none());
    assert_ne!(
        registry.context_for_player(planet_player),
        registry.context_for_player(dungeon_player)
    );
}

#[test]
fn planet_spawn_can_walk_the_generated_approach_to_the_portal() {
    let worlds = generate_demo_worlds().unwrap();
    let planet_id = worlds[0].instance;
    let spawn = worlds[0].map.spawn();
    let portal = worlds[0].portal.unwrap().position;
    let mut registry = registry_from_sample(worlds.to_vec());
    let player = PlayerId::new(12);
    registry
        .join_at(planet_id, player, "Walker".into(), spawn, 100)
        .unwrap();
    let input = HashMap::from([(
        player,
        MovementIntent {
            direction: [1.0, 0.0],
            jump: false,
            ..Default::default()
        },
    )]);
    let mut sectors = minimium_sectors::SectorManager::new(128, 128).unwrap();
    let mut active_tickets = HashMap::new();
    let mut transfer_tickets = HashMap::new();
    let mut sector_workers = crate::sector_streaming::SectorGenerationPool::new(1).unwrap();

    for _ in 0..180 {
        let readiness = crate::sector_streaming::update(
            &mut registry,
            &mut sectors,
            &mut active_tickets,
            &mut transfer_tickets,
            &mut sector_workers,
        );
        registry
            .get_mut(planet_id)
            .unwrap()
            .world
            .step_players_selected(&input, Some(&readiness.players), 0.05);
        if near_portal(
            registry
                .get(planet_id)
                .unwrap()
                .world
                .player(player)
                .unwrap()
                .position,
            portal,
        ) {
            break;
        }
    }

    let position = registry
        .get(planet_id)
        .unwrap()
        .world
        .player(player)
        .unwrap()
        .position;
    assert!(
        near_portal(position, portal),
        "player stopped at {position:?}"
    );
}

#[test]
fn portal_transfer_waits_for_readiness_and_round_trips_to_saved_location() {
    let worlds = generate_demo_worlds().unwrap();
    let planet_id = worlds[0].instance;
    let dungeon_id = worlds[1].instance;
    let planet_portal = worlds[0].portal.unwrap();
    let dungeon_portal = worlds[1].portal.unwrap();
    let mut registry = registry_from_sample(worlds.to_vec());
    let player = PlayerId::new(17);
    registry
        .join_at(
            planet_id,
            player,
            "Traveler".into(),
            worlds[0].map.spawn(),
            73,
        )
        .unwrap();
    let planet_context = registry.context_for_player(player).unwrap();
    let input = HashMap::from([(
        player,
        MovementIntent {
            direction: [1.0, 0.0],
            jump: false,
            ..Default::default()
        },
    )]);
    let mut sectors = minimium_sectors::SectorManager::new(128, 128).unwrap();
    let mut active_tickets = HashMap::new();
    let mut transfer_tickets = HashMap::new();
    let mut sector_workers = crate::sector_streaming::SectorGenerationPool::new(1).unwrap();
    for _ in 0..180 {
        let readiness = crate::sector_streaming::update(
            &mut registry,
            &mut sectors,
            &mut active_tickets,
            &mut transfer_tickets,
            &mut sector_workers,
        );
        registry
            .get_mut(planet_id)
            .unwrap()
            .world
            .step_players_selected(&input, Some(&readiness.players), 0.05);
        if near_portal(
            registry
                .get(planet_id)
                .unwrap()
                .world
                .player(player)
                .unwrap()
                .position,
            planet_portal.position,
        ) {
            break;
        }
    }
    assert!(near_portal(
        registry
            .for_player(player)
            .unwrap()
            .world
            .player(player)
            .unwrap()
            .position,
        planet_portal.position
    ));

    let cancelled = registry
        .prepare_entity_transfer(
            player,
            planet_context,
            dungeon_id,
            dungeon_portal.destination,
            None,
        )
        .unwrap();
    assert!(!registry.mark_transfer_ready(
        player,
        cancelled.transition_id,
        WorldContext::new(
            cancelled.target_context.instance(),
            cancelled.target_context.content_revision(),
            minimium_world::world::MembershipEpoch::new(999).unwrap(),
        ),
    ));
    assert!(registry.cancel_transfer(player, cancelled.transition_id));
    assert_eq!(registry.instance_for_player(player), Some(planet_id));
    assert!(!registry.cancel_transfer(player, cancelled.transition_id));

    let offer = registry
        .prepare_entity_transfer(
            player,
            planet_context,
            dungeon_id,
            dungeon_portal.destination,
            None,
        )
        .unwrap();
    assert!(registry.mark_transfer_ready(player, offer.transition_id, offer.target_context));
    assert!(registry.commit_ready_transfers(&HashSet::new()).is_empty());
    let committed = registry.commit_ready_transfers(&HashSet::from([player]));
    assert_eq!(committed.len(), 1);
    let return_location = committed[0].source_location;
    assert_eq!(committed[0].health, 73);
    assert_eq!(registry.instance_for_player(player), Some(dungeon_id));
    assert_eq!(
        registry
            .for_player(player)
            .unwrap()
            .world
            .player(player)
            .unwrap()
            .position,
        planet_portal.destination
    );
    assert!(!registry.context_matches(player, planet_context));
    assert_eq!(
        return_location.instance(),
        planet_id,
        "the source location is committed with the transfer"
    );

    let route = demo_dungeon_room_route();
    let mut dungeon_sectors = minimium_sectors::SectorManager::new(256, 256).unwrap();
    let mut dungeon_tickets = HashMap::new();
    let mut dungeon_transfers = HashMap::new();
    for segment in route.windows(2) {
        let direction = [
            f32::from(segment[1][0]) - f32::from(segment[0][0]),
            f32::from(segment[1][1]) - f32::from(segment[0][1]),
        ];
        let input = HashMap::from([(
            player,
            MovementIntent {
                direction,
                jump: false,
                ..Default::default()
            },
        )]);
        for _ in 0..4 {
            let readiness = crate::sector_streaming::update(
                &mut registry,
                &mut dungeon_sectors,
                &mut dungeon_tickets,
                &mut dungeon_transfers,
                &mut sector_workers,
            );
            registry
                .get_mut(dungeon_id)
                .unwrap()
                .world
                .step_players_selected(&input, Some(&readiness.players), 0.05);
        }
    }
    let dungeon_position = registry
        .for_player(player)
        .unwrap()
        .world
        .player(player)
        .unwrap()
        .position;
    assert!(
        near_portal(dungeon_position, dungeon_portal.position),
        "player stopped at {dungeon_position:?}"
    );
    let dungeon_context = registry.context_for_player(player).unwrap();
    let return_offer = registry
        .prepare_entity_transfer(
            player,
            dungeon_context,
            planet_id,
            planet_portal.destination,
            Some(return_location),
        )
        .unwrap();
    let expected_return = return_location
        .position()
        .coordinates()
        .map(|value| value as f32 / 1000.0);
    assert_eq!(return_offer.destination, expected_return);
    assert!(registry.mark_transfer_ready(
        player,
        return_offer.transition_id,
        return_offer.target_context
    ));
    let returned = registry.commit_ready_transfers(&HashSet::from([player]));
    assert_eq!(returned.len(), 1);
    assert_eq!(registry.instance_for_player(player), Some(planet_id));
    assert_eq!(returned[0].destination, expected_return);
    assert!(registry.context_matches(player, returned[0].context));
    assert!(!registry.context_matches(player, dungeon_context));
}

fn demo_dungeon_room_route() -> Vec<[u16; 2]> {
    let generation =
        GenerationSpec::new(WorldSeed::new(2), GenerationRevision::new(5).unwrap(), 10).unwrap();
    let blueprint = generate_dungeon(generation, DungeonSpec::new(48, 40, 4, 5, 1).unwrap());
    let mut route = vec![blueprint.spawn()];
    for room in blueprint.rooms().iter().skip(1) {
        let start = *route.last().unwrap();
        route.extend(
            dungeon_path(&blueprint, start, room.center)
                .into_iter()
                .skip(1),
        );
    }
    assert_eq!(route.last().copied(), Some(blueprint.entrance()));
    route
}

fn dungeon_path(
    blueprint: &minimium_procgen::DungeonBlueprint,
    start: [u16; 2],
    destination: [u16; 2],
) -> Vec<[u16; 2]> {
    let (width, height) = blueprint.dimensions();
    let index = |[x, y]: [u16; 2]| usize::from(y) * usize::from(width) + usize::from(x);
    let mut previous = vec![None; usize::from(width) * usize::from(height)];
    let start_index = index(start);
    previous[start_index] = Some(start_index);
    let mut pending = std::collections::VecDeque::from([start]);
    while let Some([x, y]) = pending.pop_front() {
        if [x, y] == destination {
            break;
        }
        let neighbors = [
            x.checked_sub(1).map(|next_x| [next_x, y]),
            (x + 1 < width).then_some([x + 1, y]),
            y.checked_sub(1).map(|next_y| [x, next_y]),
            (y + 1 < height).then_some([x, y + 1]),
        ];
        for neighbor in neighbors.into_iter().flatten() {
            let neighbor_index = index(neighbor);
            if blueprint.tile(neighbor[0], neighbor[1]) != Some(DungeonTile::Void)
                && previous[neighbor_index].is_none()
            {
                previous[neighbor_index] = Some(index([x, y]));
                pending.push_back(neighbor);
            }
        }
    }
    let mut path = vec![destination];
    let mut cursor = index(destination);
    while cursor != start_index {
        cursor = previous[cursor].expect("generated dungeon rooms are connected");
        path.push([
            (cursor % usize::from(width)) as u16,
            (cursor / usize::from(width)) as u16,
        ]);
    }
    path.reverse();
    path
}

#[test]
fn sector_state_revisions_are_monotonic_and_topology_bound() {
    let worlds = generate_demo_worlds().unwrap();
    let planet_id = worlds[0].instance;
    let mut registry = registry_from_sample(worlds.to_vec());
    let mut budget = crate::storage::SnapshotBudget::default();
    let key = SectorKey::planet(PlanetFace::PositiveY, 3, 4).unwrap();
    let first = WorldSectorState::new(key, SectorRevision::new(1).unwrap(), Vec::new()).unwrap();
    let stale = WorldSectorState::new(key, SectorRevision::new(1).unwrap(), Vec::new()).unwrap();
    let next = WorldSectorState::new(key, SectorRevision::new(2).unwrap(), Vec::new()).unwrap();
    assert_eq!(
        registry.commit_sector_state(planet_id, first, &mut budget),
        Ok(())
    );
    assert_eq!(
        registry.commit_sector_state(planet_id, stale, &mut budget),
        Err(RegistryError::StaleSectorRevision)
    );
    assert_eq!(
        registry.commit_sector_state(planet_id, next, &mut budget),
        Ok(())
    );

    let dungeon_key = WorldSectorState::new(
        SectorKey::dungeon(0, 0, 0),
        SectorRevision::new(1).unwrap(),
        Vec::new(),
    )
    .unwrap();
    assert_eq!(
        registry.commit_sector_state(planet_id, dungeon_key, &mut budget),
        Err(RegistryError::InvalidSectorState)
    );
}

#[test]
fn spawned_component_entity_uses_the_budgeted_entity_effect_path() {
    let worlds = generate_demo_worlds().unwrap();
    let instance = worlds[0].instance;
    let mut owner = registry_from_sample(worlds.to_vec());
    let mut budget = crate::storage::SnapshotBudget::default();
    let mut components = ComponentRegistry::default();
    let kind = ComponentTypeId::new(1).unwrap();
    let schema = SchemaVersion::new(1).unwrap();
    components
        .register(ComponentDescriptor {
            id: kind,
            name: "demo.marker",
            schema,
            replication: ReplicationPolicy::ServerOnly,
            visibility: minimium_world::entity::ReplicationVisibility::Global,
        })
        .unwrap();
    let sector = SectorKey::planet(PlanetFace::PositiveY, 3, 4).unwrap();
    owner
        .apply_entity_effect_batch(
            &[crate::modules::WorldEffect::SpawnEntity {
                world: instance,
                sector,
                components: vec![crate::modules::ComponentValue {
                    kind,
                    schema,
                    bytes: vec![1, 2, 3],
                }],
            }],
            &mut budget,
            &components,
        )
        .unwrap();
    let entity = owner.get(instance).unwrap().entities.iter().next().unwrap();
    assert_eq!(
        owner
            .get(instance)
            .unwrap()
            .entities
            .get(entity, kind)
            .unwrap()
            .bytes,
        vec![1, 2, 3]
    );
    assert!(owner.sector_states().is_empty());
}
