use super::*;

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
