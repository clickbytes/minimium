use super::*;

mod entities;
mod header;

use entities::{parse_component, parse_entity, parse_entity_counter, parse_world_initialization};
use header::parse_snapshot_header;
use std::collections::HashMap;

pub(super) fn load(
    data_dir: &Path,
    worlds: &[(WorldInstanceId, WorldMap, WorldManifest)],
    components_registry: &minimium_world::entity::ComponentRegistry,
) -> io::Result<LoadedState> {
    let path = data_dir.join("world.snapshot");
    if worlds.len() > crate::MAX_SNAPSHOT_WORLDS {
        return Err(invalid_world_snapshot("snapshot world count exceeds limit"));
    }
    let bytes = match minimium_platform::files::read_bounded_file(&path, MAX_WORLD_SNAPSHOT_BYTES) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(LoadedState::empty()),
        Err(error) => return Err(error),
    };
    let source = String::from_utf8(bytes)
        .map_err(|_| invalid_world_snapshot("snapshot is not valid UTF-8"))?;
    let mut lines = source.lines();
    let header = lines
        .next()
        .ok_or_else(|| invalid_world_snapshot("snapshot is empty"))?;
    let parsed_header = parse_snapshot_header(header, worlds)?;
    let source_version = parsed_header.source_version;

    let mut store = PlayerStore::default();
    let mut saved_world_instances = HashSet::new();
    let (primary_instance, _, _) = worlds
        .first()
        .ok_or_else(|| invalid_world_snapshot("no primary world is loaded"))?;
    saved_world_instances.insert(*primary_instance);
    let mut player_records_started = false;
    let mut sector_records_started = false;
    let mut sector_limits = SectorRecordLimits::default();
    let mut saved_clock = None;
    let mut entity_counters = Vec::new();
    let mut world_initialization = Vec::new();
    let mut entities: Vec<crate::PersistedEntity> = Vec::new();
    let mut entity_indices = HashMap::new();
    let mut component_ids = HashSet::new();
    let mut component_bytes = 0usize;
    let mut component_payload_reservations = Vec::new();
    let mut entity_records_started = false;
    let mut lines = lines.peekable();
    while let Some(line) = lines.next() {
        if line == "CLOCK" || line.starts_with("CLOCK\t") {
            if saved_clock.is_some() {
                return Err(invalid_world_snapshot("duplicate CLOCK record"));
            }
            if player_records_started || sector_records_started {
                return Err(invalid_world_snapshot(
                    "CLOCK record appears after state records",
                ));
            }
            saved_clock = Some(parse_saved_clock(line)?);
            continue;
        }
        if line.starts_with("WORLD\t") {
            if player_records_started || sector_records_started || saved_clock.is_some() {
                return Err(invalid_world_snapshot(
                    "world manifest appears after clock or state records",
                ));
            }
            parse_world_record(line, worlds, &mut saved_world_instances)?;
            continue;
        }
        if line.starts_with("SECTOR\t") {
            if player_records_started || entity_records_started {
                return Err(invalid_world_snapshot(
                    "sector state appears after player records",
                ));
            }
            if saved_clock.is_none() {
                return Err(invalid_world_snapshot("missing CLOCK record in snapshot"));
            }
            sector_records_started = true;
            parse_sector_record(
                line,
                &mut lines,
                worlds,
                &saved_world_instances,
                &mut store,
                &mut sector_limits,
            )?;
            continue;
        }
        if line.starts_with("ENTITY_COUNTER\t") {
            if saved_clock.is_none() {
                return Err(invalid_world_snapshot(
                    "entity counter appears before CLOCK",
                ));
            }
            if player_records_started || entity_records_started {
                return Err(invalid_world_snapshot(
                    "entity counter appears after entity or player records",
                ));
            }
            let (world, next_id) = parse_entity_counter(line, &saved_world_instances)?;
            if entity_counters
                .iter()
                .any(|(instance, _)| *instance == world)
            {
                return Err(invalid_world_snapshot("duplicate entity counter"));
            }
            entity_counters.push((world, next_id));
            continue;
        }
        if line.starts_with("WORLD_INIT\t") {
            if saved_clock.is_none() {
                return Err(invalid_world_snapshot(
                    "world initialization marker appears before CLOCK",
                ));
            }
            if player_records_started || entity_records_started {
                return Err(invalid_world_snapshot(
                    "world initialization marker appears after entity or player records",
                ));
            }
            let (world, initialized) = parse_world_initialization(line, &saved_world_instances)?;
            if world_initialization
                .iter()
                .any(|(instance, _)| *instance == world)
            {
                return Err(invalid_world_snapshot(
                    "duplicate world initialization marker",
                ));
            }
            world_initialization.push((world, initialized));
            continue;
        }
        if line.starts_with("ENTITY\t") {
            if saved_clock.is_none() || player_records_started {
                return Err(invalid_world_snapshot(
                    "entity record appears outside entity section",
                ));
            }
            entity_records_started = true;
            let (world, entity) = parse_entity(line, &saved_world_instances)?;
            if entity_indices.contains_key(&(world, entity))
                || entities.len() >= minimium_world::entity::MAX_PERSISTENT_ENTITIES
            {
                return Err(invalid_world_snapshot(
                    "duplicate entity or entity limit exceeded",
                ));
            }
            let record_bytes = entity_record_bytes(world, entity)?;
            store.snapshot_budget.reserve(
                crate::budget::SnapshotRecordKey::Entity(world, entity),
                record_bytes,
            )?;
            entity_indices.insert((world, entity), entities.len());
            entities.push(crate::PersistedEntity {
                world,
                id: entity,
                components: Vec::new(),
            });
            continue;
        }
        if line.starts_with("COMPONENT\t") {
            if saved_clock.is_none() || player_records_started || !entity_records_started {
                return Err(invalid_world_snapshot(
                    "component record appears outside entity section",
                ));
            }
            let (world, entity, component) = parse_component(line)?;
            components_registry
                .upgrade(component.kind, component.schema, &component.bytes)
                .map_err(|error| {
                    invalid_world_snapshot(&format!("component schema is incompatible: {error:?}"))
                })?;
            let index = entity_indices
                .get(&(world, entity))
                .copied()
                .ok_or_else(|| {
                    invalid_world_snapshot("component references an unrecorded entity")
                })?;
            let owner = &mut entities[index];
            if !component_ids.insert((world, entity, component.kind))
                || owner.components.len() >= minimium_world::entity::MAX_COMPONENTS_PER_ENTITY
            {
                return Err(invalid_world_snapshot(
                    "duplicate component or component limit exceeded",
                ));
            }
            component_bytes = component_bytes.saturating_add(component.bytes.len());
            if component_bytes > minimium_world::entity::MAX_COMPONENT_PAYLOAD_BYTES {
                return Err(invalid_world_snapshot("component payload budget exceeded"));
            }
            let blob = minimium_world::entity::ComponentBlob {
                schema: component.schema,
                bytes: component.bytes.clone(),
            };
            let record_bytes = component_record_bytes(world, entity, component.kind, &blob)?;
            store.snapshot_budget.reserve(
                crate::budget::SnapshotRecordKey::Component(world, entity, component.kind),
                record_bytes,
            )?;
            component_payload_reservations.push((
                crate::budget::SnapshotRecordKey::Component(world, entity, component.kind),
                component.bytes.len(),
            ));
            owner.components.push(component);
            continue;
        }
        if !line.starts_with("PLAYER\t") {
            return Err(invalid_world_snapshot("unknown snapshot record"));
        }
        if saved_clock.is_none() {
            return Err(invalid_world_snapshot("missing CLOCK record in snapshot"));
        }
        player_records_started = true;
        parse_player_record(line, worlds, &saved_world_instances, &mut store)?;
    }
    if saved_clock.is_none() {
        return Err(invalid_world_snapshot("missing CLOCK record in snapshot"));
    }
    if entity_counters.len() != saved_world_instances.len()
        || world_initialization.len() != saved_world_instances.len()
    {
        return Err(invalid_world_snapshot("missing per-world entity metadata"));
    }
    store
        .snapshot_budget
        .reserve_component_payloads(component_payload_reservations)?;
    for (world, entity) in entity_indices.keys() {
        let counter = entity_counters
            .iter()
            .find(|(instance, _)| instance == world)
            .map(|(_, next)| *next)
            .unwrap_or(0);
        if entity.get() > counter {
            return Err(invalid_world_snapshot("entity ID exceeds saved counter"));
        }
    }
    Ok(LoadedState::from_snapshot(
        store,
        saved_clock,
        source_version,
        entity_counters,
        world_initialization,
        entities,
    ))
}

fn parse_world_record(
    line: &str,
    worlds: &[(WorldInstanceId, WorldMap, WorldManifest)],
    saved_world_instances: &mut HashSet<WorldInstanceId>,
) -> io::Result<()> {
    let saved_world = parse_saved_world(line)?;
    validate_saved_world(&saved_world, worlds)?;
    if !saved_world_instances.insert(saved_world.instance) {
        return Err(invalid_world_snapshot("duplicate saved world instance"));
    }
    Ok(())
}

#[derive(Default)]
struct SectorRecordLimits {
    addresses: HashSet<(WorldInstanceId, SectorKey)>,
    object_count: usize,
    object_bytes: usize,
}

fn parse_sector_record(
    line: &str,
    lines: &mut std::iter::Peekable<std::str::Lines<'_>>,
    worlds: &[(WorldInstanceId, WorldMap, WorldManifest)],
    saved_world_instances: &HashSet<WorldInstanceId>,
    store: &mut PlayerStore,
    limits: &mut SectorRecordLimits,
) -> io::Result<()> {
    let (instance, state) = parse_saved_sector(line, lines)?;
    if !saved_world_instances.contains(&instance) {
        return Err(invalid_world_snapshot(
            "sector state references an unrecorded world instance",
        ));
    }
    let Some((_, _, manifest)) = worlds
        .iter()
        .find(|(candidate, _, _)| *candidate == instance)
    else {
        return Err(invalid_world_snapshot(
            "sector state references an unloaded world instance",
        ));
    };
    let key_matches = matches!(
        (manifest.topology(), state.key()),
        (WorldTopology::Planet, SectorKey::Planet(_))
            | (WorldTopology::Dungeon, SectorKey::Dungeon(_))
    );
    if !key_matches {
        return Err(invalid_world_snapshot(
            "sector address does not match its world topology",
        ));
    }
    if store.sector_states.len() >= MAX_PERSISTED_WORLD_SECTORS {
        return Err(invalid_world_snapshot(
            "snapshot has too many sector states",
        ));
    }
    if !limits.addresses.insert((instance, state.key())) {
        return Err(invalid_world_snapshot("duplicate saved sector state"));
    }
    limits.object_count = limits.object_count.saturating_add(state.objects().len());
    limits.object_bytes = limits.object_bytes.saturating_add(
        state
            .objects()
            .iter()
            .map(|object| object.data().len())
            .sum::<usize>(),
    );
    if limits.object_count > MAX_PERSISTED_WORLD_OBJECT_STATES
        || limits.object_bytes > MAX_PERSISTED_WORLD_OBJECT_STATE_BYTES
    {
        return Err(invalid_world_snapshot(
            "snapshot has too much persisted sector object state",
        ));
    }
    store.sector_states.push((instance, state));
    Ok(())
}

fn parse_player_record(
    line: &str,
    worlds: &[(WorldInstanceId, WorldMap, WorldManifest)],
    saved_world_instances: &HashSet<WorldInstanceId>,
    store: &mut PlayerStore,
) -> io::Result<()> {
    if store.profile_by_player.len() >= MAX_STORED_PLAYERS {
        return Err(invalid_world_snapshot("snapshot has too many players"));
    }
    let mut fields = line.split('\t');
    if fields.next() != Some("PLAYER") {
        return Err(invalid_world_snapshot("invalid player record"));
    }
    let id = PlayerId::new(parse_saved_id(fields.next())?);
    let profile_id = PlayerProfileId::from_hex(
        fields
            .next()
            .ok_or_else(|| invalid_world_snapshot("missing player profile id"))?,
    )
    .map_err(|_| invalid_world_snapshot("invalid player profile id"))?;
    let name = fields
        .next()
        .ok_or_else(|| invalid_world_snapshot("missing player name"))?;
    let name =
        StoredName::new(name).ok_or_else(|| invalid_world_snapshot("invalid saved player name"))?;
    let location = parse_location(&mut fields)?;
    let health = parse_saved_health(fields.next())?;
    let return_location = parse_location(&mut fields)?;
    if !saved_world_instances.contains(&location.instance())
        || !saved_world_instances.contains(&return_location.instance())
    {
        return Err(invalid_world_snapshot(
            "player location references an unrecorded world instance",
        ));
    }
    if fields.next().is_some() {
        return Err(invalid_world_snapshot("player record has extra fields"));
    }
    let position = position_from_location(location);
    let current_map = map_for_world(worlds, location.instance())?;
    let current_gravity = gravity_for_world(worlds, location.instance())?;
    if !current_map.can_place_player_with_gravity(position, current_gravity) {
        return Err(invalid_world_snapshot(
            "saved player position overlaps map geometry",
        ));
    }
    let return_map = map_for_world(worlds, return_location.instance())?;
    let return_gravity = gravity_for_world(worlds, return_location.instance())?;
    if !return_map
        .can_place_player_with_gravity(position_from_location(return_location), return_gravity)
    {
        return Err(invalid_world_snapshot(
            "saved return point overlaps map geometry",
        ));
    }

    store.reserve_profile_record(profile_id)?;

    let shard = usize::from(profile_id.as_bytes()[0]);
    let duplicate_profile = Arc::make_mut(&mut store.players[shard])
        .insert(
            profile_id,
            StoredPlayer {
                id,
                name,
                position,
                health,
                location,
                return_location,
            },
        )
        .is_some();
    let duplicate_player = store.profile_by_player.insert(id, profile_id).is_some();
    if duplicate_profile || duplicate_player {
        return Err(invalid_world_snapshot("duplicate saved player identity"));
    }
    Ok(())
}

fn map_for_world(
    worlds: &[(WorldInstanceId, WorldMap, WorldManifest)],
    instance: WorldInstanceId,
) -> io::Result<&WorldMap> {
    worlds
        .iter()
        .find(|(candidate, _, _)| *candidate == instance)
        .map(|(_, map, _)| map)
        .ok_or_else(|| invalid_world_snapshot("snapshot references an unloaded world instance"))
}

fn gravity_for_world(
    worlds: &[(WorldInstanceId, WorldMap, WorldManifest)],
    instance: WorldInstanceId,
) -> io::Result<minimium_world::world::CharacterGravity> {
    let manifest = worlds
        .iter()
        .find(|(candidate, _, _)| *candidate == instance)
        .map(|(_, _, manifest)| manifest)
        .ok_or_else(|| invalid_world_snapshot("snapshot references an unloaded world instance"))?;
    Ok(match manifest.topology() {
        WorldTopology::Planet => {
            minimium_world::world::CharacterGravity::Radial { center: [0.0; 3] }
        }
        WorldTopology::Dungeon => minimium_world::world::CharacterGravity::FixedY,
    })
}

fn parse_saved_id(value: Option<&str>) -> io::Result<u64> {
    value
        .ok_or_else(|| invalid_world_snapshot("missing player id"))?
        .parse::<u64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| invalid_world_snapshot("invalid player id"))
}

fn parse_saved_health(value: Option<&str>) -> io::Result<u16> {
    let health = value
        .ok_or_else(|| invalid_world_snapshot("missing player health"))?
        .parse::<u16>()
        .map_err(|_| invalid_world_snapshot("invalid player health"))?;
    if !(1..=PLAYER_MAX_HEALTH).contains(&health) {
        return Err(invalid_world_snapshot("player health is out of range"));
    }
    Ok(health)
}
