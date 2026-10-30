use super::{
    invalid_world_snapshot,
    player_store::{PlayerStore, SaveSnapshot, StoredName, StoredPlayer},
};
use minimium_core::ids::PlayerId;
use minimium_core::{
    identity::PlayerProfileId,
    world::{WorldMap, PLAYER_MAX_HEALTH},
};
use minimium_platform::files::atomic_replace_file;
use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::Path,
    sync::Arc,
};

pub(crate) const SAVE_VERSION: u8 = 3;
pub(super) const MAX_STORED_PLAYERS: usize = 100_000;
const MAX_WORLD_SNAPSHOT_BYTES: usize = 16 * 1024 * 1024;
const MAX_SAVED_COORDINATE: f32 = 1_000_000.0;
pub(super) const PLAYER_STORE_SHARDS: usize = 256;

pub(super) fn load(data_dir: &Path, map: &WorldMap) -> io::Result<PlayerStore> {
    let path = data_dir.join("world.snapshot");
    let mut source = String::new();
    match File::open(path) {
        Ok(file) => {
            file.take((MAX_WORLD_SNAPSHOT_BYTES + 1) as u64)
                .read_to_string(&mut source)?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(PlayerStore::default()),
        Err(error) => return Err(error),
    }
    if source.len() > MAX_WORLD_SNAPSHOT_BYTES {
        return Err(invalid_world_snapshot("snapshot exceeds size limit"));
    }
    let mut lines = source.lines();
    let header = lines
        .next()
        .ok_or_else(|| invalid_world_snapshot("snapshot is empty"))?;
    let mut header_fields = header.split('\t');
    let expected_fingerprint = format!("{:016x}", map.fingerprint());
    let signature = header_fields.next();
    let version = header_fields.next();
    let fingerprint = header_fields.next();
    if signature != Some("MINIMIUM_WORLD") || header_fields.next().is_some() {
        return Err(invalid_world_snapshot("invalid snapshot header"));
    }
    if version == Some("1") {
        return Err(invalid_world_snapshot("snapshot version 1 has no client profile IDs; back up or remove the old snapshot before starting"));
    }
    if version != Some("2") && version != Some("3") {
        return Err(invalid_world_snapshot("unsupported snapshot version"));
    }
    if fingerprint != Some(expected_fingerprint.as_str()) {
        return Err(invalid_world_snapshot(
            "snapshot map fingerprint does not match",
        ));
    }

    let mut store = PlayerStore::default();
    for line in lines {
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
        if fields.next().is_some() || !map.can_place_player(position) {
            return Err(invalid_world_snapshot(
                "saved player position overlaps map geometry",
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
                },
            )
            .is_some();
        let duplicate_player = store.profile_by_player.insert(id, profile_id).is_some();
        if duplicate_profile || duplicate_player {
            return Err(invalid_world_snapshot("duplicate saved player identity"));
        }
    }
    Ok(store)
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
    let result = (|| {
        let mut file = File::create(&temporary_path)?;
        writeln!(
            file,
            "MINIMIUM_WORLD\t{SAVE_VERSION}\t{:016x}",
            snapshot.map_fingerprint
        )?;
        for (_, profile_id, player) in players {
            writeln!(
                file,
                "{}\t{}\t{}\t{:.6}\t{:.6}\t{:.6}\t{}",
                player.id,
                profile_id.to_hex(),
                player.name.as_str(),
                player.position[0],
                player.position[1],
                player.position[2],
                player.health
            )?;
        }
        file.sync_all()?;
        atomic_replace_file(&temporary_path, &path)?;
        #[cfg(unix)]
        File::open(data_dir)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary_path);
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
