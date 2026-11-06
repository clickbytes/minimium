use super::*;

mod header;

use header::parse_snapshot_header;

pub(super) fn load(
    data_dir: &Path,
    worlds: &[(WorldInstanceId, WorldMap, WorldManifest)],
) -> io::Result<LoadedState> {
    let path = data_dir.join("world.snapshot");
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
    let legacy_world = parsed_header.legacy_world;

    let mut store = PlayerStore::default();
    let mut saved_world_instances = HashSet::new();
    if source_version >= 5 {
        let (primary_instance, _, _) = worlds
            .first()
            .ok_or_else(|| invalid_world_snapshot("no primary world is loaded"))?;
        saved_world_instances.insert(*primary_instance);
    }
    let mut player_records_started = false;
    let mut sector_records_started = false;
    let mut sector_limits = SectorRecordLimits::default();
    let mut saved_clock = None;
    let mut lines = lines.peekable();
    while let Some(line) = lines.next() {
        if line == "CLOCK" || line.starts_with("CLOCK\t") {
            if source_version < 8 {
                return Err(invalid_world_snapshot(
                    "CLOCK record is not allowed in a legacy snapshot",
                ));
            }
            if saved_clock.is_some() {
                return Err(invalid_world_snapshot("duplicate CLOCK record"));
            }
            if player_records_started || sector_records_started {
                return Err(invalid_world_snapshot(
                    "CLOCK record appears after state records",
                ));
            }
            saved_clock = Some(parse_saved_clock(line, source_version)?);
            continue;
        }
        if source_version >= 5 && line.starts_with("WORLD\t") {
            if player_records_started || sector_records_started || saved_clock.is_some() {
                return Err(invalid_world_snapshot(
                    "world manifest appears after clock or state records",
                ));
            }
            parse_world_record(line, worlds, &mut saved_world_instances)?;
            continue;
        }
        if source_version >= 6 && line.starts_with("SECTOR\t") {
            if player_records_started {
                return Err(invalid_world_snapshot(
                    "sector state appears after player records",
                ));
            }
            if source_version >= 8 && saved_clock.is_none() {
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
        if source_version >= 8 && saved_clock.is_none() {
            return Err(invalid_world_snapshot("missing CLOCK record in snapshot"));
        }
        player_records_started = true;
        parse_player_record(
            line,
            source_version,
            legacy_world,
            worlds,
            &saved_world_instances,
            &mut store,
        )?;
    }
    if source_version >= 8 && saved_clock.is_none() {
        return Err(invalid_world_snapshot("missing CLOCK record in snapshot"));
    }
    Ok(LoadedState::from_snapshot(
        store,
        saved_clock,
        source_version,
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
    source_version: u8,
    legacy_world: WorldInstanceId,
    worlds: &[(WorldInstanceId, WorldMap, WorldManifest)],
    saved_world_instances: &HashSet<WorldInstanceId>,
    store: &mut PlayerStore,
) -> io::Result<()> {
    if store.profile_by_player.len() >= MAX_STORED_PLAYERS {
        return Err(invalid_world_snapshot("snapshot has too many players"));
    }
    let mut fields = line.split('\t');
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
    let (location, return_location, health) =
        parse_player_locations(&mut fields, source_version, legacy_world)?;
    validate_player_world_references(
        source_version,
        location,
        return_location,
        worlds,
        saved_world_instances,
    )?;
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

fn parse_player_locations(
    fields: &mut std::str::Split<'_, char>,
    source_version: u8,
    legacy_world: WorldInstanceId,
) -> io::Result<(WorldLocation, WorldLocation, u16)> {
    if source_version >= 4 {
        parse_current_player_locations(fields)
    } else {
        parse_legacy_player_location(fields, source_version, legacy_world)
    }
}

fn parse_current_player_locations(
    fields: &mut std::str::Split<'_, char>,
) -> io::Result<(WorldLocation, WorldLocation, u16)> {
    let location = parse_location(fields)?;
    let health = parse_saved_health(fields.next())?;
    let return_location = parse_location(fields)?;
    Ok((location, return_location, health))
}

fn parse_legacy_player_location(
    fields: &mut std::str::Split<'_, char>,
    source_version: u8,
    legacy_world: WorldInstanceId,
) -> io::Result<(WorldLocation, WorldLocation, u16)> {
    let position = [
        parse_saved_coordinate(fields.next())?,
        parse_saved_coordinate(fields.next())?,
        parse_saved_coordinate(fields.next())?,
    ];
    let health = if source_version == 3 {
        parse_saved_health(fields.next())?
    } else {
        PLAYER_MAX_HEALTH
    };
    let location = legacy_location(legacy_world, position);
    Ok((location, location, health))
}

fn validate_player_world_references(
    source_version: u8,
    location: WorldLocation,
    return_location: WorldLocation,
    worlds: &[(WorldInstanceId, WorldMap, WorldManifest)],
    saved_world_instances: &HashSet<WorldInstanceId>,
) -> io::Result<()> {
    if source_version == 4 {
        let primary_instance = worlds
            .first()
            .map(|(instance, _, _)| *instance)
            .ok_or_else(|| invalid_world_snapshot("no primary world is loaded"))?;
        if location.instance() != primary_instance || return_location.instance() != primary_instance
        {
            return Err(invalid_world_snapshot(
                "snapshot version 4 does not pin target world manifests",
            ));
        }
    }
    if source_version >= 5
        && (!saved_world_instances.contains(&location.instance())
            || !saved_world_instances.contains(&return_location.instance()))
    {
        return Err(invalid_world_snapshot(
            "player location references an unrecorded world instance",
        ));
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

fn parse_saved_coordinate(value: Option<&str>) -> io::Result<f32> {
    let coordinate = value
        .ok_or_else(|| invalid_world_snapshot("missing player coordinate"))?
        .parse::<f32>()
        .map_err(|_| invalid_world_snapshot("invalid player coordinate"))?;
    if !coordinate.is_finite() || coordinate.abs() > MAX_SAVED_COORDINATE {
        return Err(invalid_world_snapshot("player coordinate is out of range"));
    }
    Ok(coordinate)
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
