use super::{
    invalid_world_snapshot,
    player_store::{PlayerStore, SaveSnapshot, SavedWorldManifest, StoredName, StoredPlayer},
    LoadedState,
};
use minimium_core::{
    identity::PlayerProfileId,
    ids::PlayerId,
    world::{
        GenerationRevision, GenerationSpec, GeneratorDigest, PlanetDefinition, Position3Mm,
        SectorKey, WorldContentRevision, WorldDefinitionId, WorldInstanceId, WorldLocation,
        WorldManifest, WorldMap, WorldOrientationQ30, WorldSeed, WorldTopology,
        MAX_PERSISTED_WORLD_OBJECT_STATES, MAX_PERSISTED_WORLD_OBJECT_STATE_BYTES,
        MAX_PERSISTED_WORLD_SECTORS, PLAYER_MAX_HEALTH,
    },
};
mod clock;
mod locations;
mod sectors;
mod worlds;

#[cfg(test)]
mod tests;
use clock::{parse_saved_clock, write_saved_clock};
use locations::{legacy_location, parse_location, position_from_location, write_location};
use sectors::{parse_saved_sector, write_saved_sector};
use worlds::{
    parse_manifest, parse_saved_world, validate_saved_world, write_manifest, write_saved_world,
};

use std::{
    collections::HashSet,
    io::{self, Write},
    path::Path,
    sync::Arc,
};

pub(crate) const SAVE_VERSION: u8 = 8;
pub(super) const MAX_STORED_PLAYERS: usize = 100_000;
const MAX_WORLD_SNAPSHOT_BYTES: usize = 16 * 1024 * 1024;
const MAX_SAVED_COORDINATE: f32 = 7_000_000.0;
const MAX_SAVED_POSITION_MM: u64 = 7_000_000_000;
pub(super) const PLAYER_STORE_SHARDS: usize = 256;

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
    let mut header_fields = header.split('\t');
    let signature = header_fields.next();
    let version = header_fields.next();
    let fingerprint = header_fields
        .next()
        .ok_or_else(|| invalid_world_snapshot("missing snapshot map fingerprint"))?;
    let fingerprint = u64::from_str_radix(fingerprint, 16)
        .map_err(|_| invalid_world_snapshot("invalid snapshot map fingerprint"))?;
    if signature != Some("MINIMIUM_WORLD") {
        return Err(invalid_world_snapshot("invalid snapshot header"));
    }
    if version == Some("1") {
        return Err(invalid_world_snapshot("snapshot version 1 has no client profile IDs; back up or remove the old snapshot before starting"));
    }
    if !matches!(version, Some("2" | "3" | "4" | "5" | "6" | "7" | "8")) {
        return Err(invalid_world_snapshot("unsupported snapshot version"));
    }
    let source_version = version
        .and_then(|value| value.parse::<u8>().ok())
        .ok_or_else(|| invalid_world_snapshot("invalid snapshot version"))?;
    let legacy_world = worlds
        .iter()
        .find(|(_, map, _)| map.fingerprint() == fingerprint)
        .ok_or_else(|| {
            invalid_world_snapshot(
                "snapshot map fingerprint does not match any loaded legacy world",
            )
        })?;
    match version {
        Some("4" | "5" | "6" | "7" | "8") => {
            let saved_manifest = parse_manifest(&mut header_fields)?;
            let Some((_, primary_map, primary_manifest)) = worlds.first() else {
                return Err(invalid_world_snapshot("no primary world is loaded"));
            };
            if header_fields.next().is_some()
                || primary_map.fingerprint() != fingerprint
                || *primary_manifest != saved_manifest
            {
                return Err(invalid_world_snapshot(
                    "snapshot primary world manifest or fingerprint does not match",
                ));
            }
        }
        Some("2" | "3") if header_fields.next().is_some() => {
            return Err(invalid_world_snapshot("invalid legacy snapshot header"));
        }
        Some("2" | "3") => {}
        _ => return Err(invalid_world_snapshot("invalid snapshot header")),
    }

    let mut store = PlayerStore::default();
    let mut saved_world_instances = HashSet::new();
    if matches!(version, Some("5" | "6" | "7" | "8")) {
        let (primary_instance, _, _) = worlds
            .first()
            .ok_or_else(|| invalid_world_snapshot("no primary world is loaded"))?;
        saved_world_instances.insert(*primary_instance);
    }
    let mut player_records_started = false;
    let mut sector_records_started = false;
    let mut saved_sector_addresses = std::collections::HashSet::new();
    let mut saved_object_count = 0_usize;
    let mut saved_object_bytes = 0_usize;
    let mut saved_clock = None;
    let mut lines = lines.peekable();
    while let Some(line) = lines.next() {
        if line == "CLOCK" || line.starts_with("CLOCK\t") {
            if source_version != 8 {
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
            saved_clock = Some(parse_saved_clock(line)?);
            continue;
        }
        if matches!(version, Some("5" | "6" | "7" | "8")) && line.starts_with("WORLD\t") {
            if player_records_started || sector_records_started || saved_clock.is_some() {
                return Err(invalid_world_snapshot(
                    "world manifest appears after clock or state records",
                ));
            }
            let saved_world = parse_saved_world(line)?;
            validate_saved_world(&saved_world, worlds)?;
            if !saved_world_instances.insert(saved_world.instance) {
                return Err(invalid_world_snapshot("duplicate saved world instance"));
            }
            continue;
        }
        if matches!(version, Some("6" | "7" | "8")) && line.starts_with("SECTOR\t") {
            if player_records_started {
                return Err(invalid_world_snapshot(
                    "sector state appears after player records",
                ));
            }
            if source_version == 8 && saved_clock.is_none() {
                return Err(invalid_world_snapshot(
                    "missing CLOCK record in v8 snapshot",
                ));
            }
            sector_records_started = true;
            let (instance, state) = parse_saved_sector(line, &mut lines)?;
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
            if !saved_sector_addresses.insert((instance, state.key())) {
                return Err(invalid_world_snapshot("duplicate saved sector state"));
            }
            saved_object_count = saved_object_count.saturating_add(state.objects().len());
            saved_object_bytes = saved_object_bytes.saturating_add(
                state
                    .objects()
                    .iter()
                    .map(|object| object.data().len())
                    .sum::<usize>(),
            );
            if saved_object_count > MAX_PERSISTED_WORLD_OBJECT_STATES
                || saved_object_bytes > MAX_PERSISTED_WORLD_OBJECT_STATE_BYTES
            {
                return Err(invalid_world_snapshot(
                    "snapshot has too much persisted sector object state",
                ));
            }
            store.sector_states.push((instance, state));
            continue;
        }
        if source_version == 8 && saved_clock.is_none() {
            return Err(invalid_world_snapshot(
                "missing CLOCK record in v8 snapshot",
            ));
        }
        player_records_started = true;
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
        let name = StoredName::new(name)
            .ok_or_else(|| invalid_world_snapshot("invalid saved player name"))?;
        let (location, return_location, health) =
            if matches!(version, Some("4" | "5" | "6" | "7" | "8")) {
                let location = parse_location(&mut fields)?;
                let health = parse_saved_health(fields.next())?;
                let return_location = parse_location(&mut fields)?;
                (location, return_location, health)
            } else {
                let position = [
                    parse_saved_coordinate(fields.next())?,
                    parse_saved_coordinate(fields.next())?,
                    parse_saved_coordinate(fields.next())?,
                ];
                let health = if version == Some("3") {
                    parse_saved_health(fields.next())?
                } else {
                    PLAYER_MAX_HEALTH
                };
                let location = legacy_location(legacy_world.0, position);
                (location, location, health)
            };
        if version == Some("4") {
            let primary_instance = worlds
                .first()
                .map(|(instance, _, _)| *instance)
                .ok_or_else(|| invalid_world_snapshot("no primary world is loaded"))?;
            if location.instance() != primary_instance
                || return_location.instance() != primary_instance
            {
                return Err(invalid_world_snapshot(
                    "snapshot version 4 does not pin target world manifests",
                ));
            }
        }
        if matches!(version, Some("5" | "6" | "7" | "8"))
            && (!saved_world_instances.contains(&location.instance())
                || !saved_world_instances.contains(&return_location.instance()))
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
    }
    if source_version == 8 && saved_clock.is_none() {
        return Err(invalid_world_snapshot(
            "missing CLOCK record in v8 snapshot",
        ));
    }
    Ok(LoadedState::from_snapshot(
        store,
        saved_clock,
        source_version,
    ))
}

pub(super) fn write(snapshot: SaveSnapshot, data_dir: &Path) -> io::Result<()> {
    let mut players: Vec<_> = snapshot
        .player_shards
        .iter()
        .flat_map(|shard| {
            shard
                .iter()
                .map(|(profile_id, player)| (player.id, *profile_id, *player))
        })
        .collect();
    players.sort_unstable_by_key(|(id, _, _)| *id);
    let path = data_dir.join("world.snapshot");
    let temporary_path = data_dir.join("world.snapshot.tmp");
    let mut bytes = Vec::new();
    {
        write!(
            bytes,
            "MINIMIUM_WORLD\t{SAVE_VERSION}\t{:016x}",
            snapshot.map_fingerprint
        )?;
        write_manifest(&mut bytes, snapshot.manifest)?;
        writeln!(bytes)?;
        for world in snapshot
            .world_manifests
            .iter()
            .filter(|world| world.instance != snapshot.primary_instance)
        {
            write_saved_world(&mut bytes, world)?;
            writeln!(bytes)?;
        }
        write_saved_clock(&mut bytes, snapshot.saved_clock)?;
        writeln!(bytes)?;
        for (instance, state) in &snapshot.sector_states {
            write_saved_sector(&mut bytes, *instance, state)?;
        }
        for (_, profile_id, player) in players {
            write!(
                bytes,
                "{}\t{}\t{}",
                player.id,
                profile_id.to_hex(),
                player.name.as_str()
            )?;
            write_location(&mut bytes, player.location)?;
            write!(bytes, "\t{}", player.health)?;
            write_location(&mut bytes, player.return_location)?;
            writeln!(bytes)?;
        }
    }
    if bytes.len() > MAX_WORLD_SNAPSHOT_BYTES {
        return Err(invalid_world_snapshot("snapshot exceeds size limit"));
    }
    minimium_platform::files::atomic_write_file(&temporary_path, &path, &bytes)
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
) -> io::Result<minimium_core::world::CharacterGravity> {
    let manifest = worlds
        .iter()
        .find(|(candidate, _, _)| *candidate == instance)
        .map(|(_, _, manifest)| manifest)
        .ok_or_else(|| invalid_world_snapshot("snapshot references an unloaded world instance"))?;
    Ok(match manifest.topology() {
        WorldTopology::Planet => {
            minimium_core::world::CharacterGravity::Radial { center: [0.0; 3] }
        }
        WorldTopology::Dungeon => minimium_core::world::CharacterGravity::FixedY,
    })
}

fn hex_digest(bytes: &[u8; 32]) -> String {
    let mut result = String::with_capacity(64);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(result, "{byte:02x}");
    }
    result
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
