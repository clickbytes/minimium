use super::*;
use std::{collections::HashMap, sync::OnceLock};

struct CountingWriter(usize);

impl io::Write for CountingWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .ok_or_else(crate::snapshot_limit_exceeded)?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct LimitedWriter<'a, W> {
    inner: &'a mut W,
    written: usize,
}

impl<W: io::Write> io::Write for LimitedWriter<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let next = self
            .written
            .checked_add(bytes.len())
            .ok_or_else(crate::snapshot_limit_exceeded)?;
        if next > MAX_WORLD_SNAPSHOT_BYTES {
            return Err(crate::snapshot_limit_exceeded());
        }
        self.inner.write_all(bytes)?;
        self.written = next;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

pub(super) fn maximum_profile_record_bytes() -> io::Result<usize> {
    static MAXIMUM: OnceLock<usize> = OnceLock::new();
    if let Some(value) = MAXIMUM.get() {
        return Ok(*value);
    }
    let measured = measure_maximum_profile_record_bytes()?;
    let _ = MAXIMUM.set(measured);
    Ok(*MAXIMUM.get().unwrap_or(&measured))
}

fn measure_maximum_profile_record_bytes() -> io::Result<usize> {
    let max_id = minimium_core::ids::PlayerId::new(u64::MAX);
    let profile = minimium_core::identity::PlayerProfileId::from_bytes(
        [u8::MAX; minimium_core::identity::PLAYER_PROFILE_ID_BYTES],
    );
    let name = StoredName::new(&"A".repeat(minimium_core::identity::MAX_PLAYER_NAME_BYTES))
        .ok_or_else(|| crate::invalid_world_snapshot("could not build maximum profile record"))?;
    let instance = minimium_world::world::WorldInstanceId::new(u128::MAX)
        .ok_or_else(|| crate::invalid_world_snapshot("could not build maximum profile record"))?;
    let orientation = minimium_world::world::WorldOrientationQ30::new([
        1_007_000_000,
        -215_000_000,
        -215_000_000,
        -215_000_000,
    ])
    .ok_or_else(|| crate::invalid_world_snapshot("could not build maximum profile record"))?;
    let position = minimium_world::world::Position3Mm::new(
        -(MAX_SAVED_POSITION_MM as i64),
        -(MAX_SAVED_POSITION_MM as i64),
        -(MAX_SAVED_POSITION_MM as i64),
    );
    let location = minimium_world::world::WorldLocation::new(instance, position, orientation);
    let player = StoredPlayer {
        id: max_id,
        name,
        position: [0.0; 3],
        health: u16::MAX,
        location,
        return_location: location,
    };
    let mut counter = CountingWriter(0);
    encode_player_record(&mut counter, max_id, profile, player)?;
    Ok(counter.0)
}

pub(super) fn sector_record_bytes(
    instance: minimium_world::world::WorldInstanceId,
    state: &minimium_world::world::WorldSectorState,
) -> io::Result<usize> {
    let mut counter = CountingWriter(0);
    write_saved_sector(&mut counter, instance, state)?;
    Ok(counter.0)
}

pub(super) fn entity_record_bytes(
    instance: minimium_world::world::WorldInstanceId,
    entity: minimium_world::entity::EntityId,
) -> io::Result<usize> {
    let mut counter = CountingWriter(0);
    encode_entity_record(&mut counter, instance, entity)?;
    Ok(counter.0)
}

pub(super) fn component_record_bytes(
    instance: minimium_world::world::WorldInstanceId,
    entity: minimium_world::entity::EntityId,
    kind: minimium_world::entity::ComponentTypeId,
    blob: &minimium_world::entity::ComponentBlob,
) -> io::Result<usize> {
    let mut counter = CountingWriter(0);
    encode_component_record(
        &mut counter,
        instance,
        entity,
        kind,
        blob.schema,
        &blob.bytes,
    )?;
    Ok(counter.0)
}

fn encode_entity_record(
    writer: &mut impl io::Write,
    instance: minimium_world::world::WorldInstanceId,
    entity: minimium_world::entity::EntityId,
) -> io::Result<()> {
    writeln!(writer, "ENTITY\t{instance}\t{}", entity.get())
}

fn encode_entity_counter(
    writer: &mut impl io::Write,
    instance: minimium_world::world::WorldInstanceId,
    next_id: u64,
) -> io::Result<()> {
    writeln!(writer, "ENTITY_COUNTER\t{instance}\t{next_id}")
}

fn encode_world_initialization(
    writer: &mut impl io::Write,
    instance: minimium_world::world::WorldInstanceId,
    initialized: bool,
) -> io::Result<()> {
    writeln!(writer, "WORLD_INIT\t{instance}\t{}", u8::from(initialized))
}

fn encode_component_record(
    writer: &mut impl io::Write,
    instance: minimium_world::world::WorldInstanceId,
    entity: minimium_world::entity::EntityId,
    kind: minimium_world::entity::ComponentTypeId,
    schema: minimium_world::entity::SchemaVersion,
    bytes: &[u8],
) -> io::Result<()> {
    write!(
        writer,
        "COMPONENT\t{instance}\t{}\t{}\t{}\t",
        entity.get(),
        kind.get(),
        schema.get()
    )?;
    for byte in bytes {
        write!(writer, "{byte:02x}")?;
    }
    writeln!(writer)
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
    let mut counter = CountingWriter(0);
    encode_snapshot(&mut counter, &snapshot, &players)?;
    if counter.0 > MAX_WORLD_SNAPSHOT_BYTES {
        return Err(crate::snapshot_limit_exceeded());
    }
    minimium_platform::files::atomic_write_with(&temporary_path, &path, |file| {
        let mut writer = LimitedWriter {
            inner: file,
            written: 0,
        };
        encode_snapshot(&mut writer, &snapshot, &players)
    })
}

fn encode_snapshot(
    writer: &mut impl io::Write,
    snapshot: &SaveSnapshot,
    players: &[(
        minimium_core::ids::PlayerId,
        minimium_core::identity::PlayerProfileId,
        StoredPlayer,
    )],
) -> io::Result<()> {
    validate_metadata_reservation(snapshot)?;
    {
        write!(
            writer,
            "MINIMIUM_WORLD\t{SAVE_VERSION}\t{:016x}",
            snapshot.map_fingerprint
        )?;
        write_manifest(writer, snapshot.manifest)?;
        writeln!(writer)?;
        for world in snapshot
            .world_manifests
            .iter()
            .filter(|world| world.instance != snapshot.primary_instance)
        {
            write_saved_world(writer, world)?;
            writeln!(writer)?;
        }
        write_saved_clock(writer, snapshot.saved_clock)?;
        writeln!(writer)?;
        for (instance, state) in &snapshot.sector_states {
            write_saved_sector(writer, *instance, state)?;
        }
        for (instance, next_id) in &snapshot.entity_counters {
            encode_entity_counter(writer, *instance, *next_id)?;
        }
        for (instance, initialized) in &snapshot.world_initialization {
            encode_world_initialization(writer, *instance, *initialized)?;
        }
        for entity in &snapshot.entities {
            encode_entity_record(writer, entity.world, entity.id)?;
            for component in &entity.components {
                encode_component_record(
                    writer,
                    entity.world,
                    entity.id,
                    component.kind,
                    component.schema,
                    &component.bytes,
                )?;
            }
        }
        for (id, profile_id, player) in players.iter().copied() {
            encode_player_record(writer, id, profile_id, player)?;
        }
    }
    Ok(())
}

fn validate_metadata_reservation(snapshot: &SaveSnapshot) -> io::Result<()> {
    if snapshot.world_manifests.len() > crate::budget::MAX_SNAPSHOT_WORLDS
        || snapshot.entity_counters.len() != snapshot.world_manifests.len()
        || snapshot.world_initialization.len() != snapshot.world_manifests.len()
    {
        return Err(crate::invalid_world_snapshot(
            "snapshot world metadata is incomplete or exceeds limit",
        ));
    }
    let mut per_world = HashMap::with_capacity(snapshot.world_manifests.len());
    let expected_worlds: std::collections::HashSet<_> = snapshot
        .world_manifests
        .iter()
        .map(|world| world.instance)
        .collect();
    if expected_worlds.len() != snapshot.world_manifests.len()
        || !expected_worlds.contains(&snapshot.primary_instance)
    {
        return Err(crate::invalid_world_snapshot(
            "duplicate or missing primary world metadata",
        ));
    }
    for world in &snapshot.world_manifests {
        if per_world.insert(world.instance, 0usize).is_some() {
            return Err(crate::invalid_world_snapshot(
                "duplicate snapshot world metadata",
            ));
        }
        if world.instance != snapshot.primary_instance {
            let mut record = CountingWriter(0);
            super::write_saved_world(&mut record, world)?;
            writeln!(&mut record)?;
            add_world_metadata(&mut per_world, world.instance, record.0)?;
        }
    }
    let mut counter_worlds = std::collections::HashSet::new();
    for (world, next_id) in &snapshot.entity_counters {
        if !expected_worlds.contains(world) || !counter_worlds.insert(*world) {
            return Err(crate::invalid_world_snapshot(
                "duplicate or unknown entity counter",
            ));
        }
        let mut record = CountingWriter(0);
        encode_entity_counter(&mut record, *world, *next_id)?;
        add_world_metadata(&mut per_world, *world, record.0)?;
    }
    let mut initialization_worlds = std::collections::HashSet::new();
    for (world, initialized) in &snapshot.world_initialization {
        if !expected_worlds.contains(world) || !initialization_worlds.insert(*world) {
            return Err(crate::invalid_world_snapshot(
                "duplicate or unknown initialization marker",
            ));
        }
        let mut record = CountingWriter(0);
        encode_world_initialization(&mut record, *world, *initialized)?;
        add_world_metadata(&mut per_world, *world, record.0)?;
    }
    if per_world
        .values()
        .any(|bytes| *bytes > crate::budget::MAX_WORLD_METADATA_BYTES)
    {
        return Err(crate::snapshot_limit_exceeded());
    }
    let mut fixed = CountingWriter(0);
    write!(
        &mut fixed,
        "MINIMIUM_WORLD\t{SAVE_VERSION}\t{:016x}",
        snapshot.map_fingerprint
    )?;
    write_manifest(&mut fixed, snapshot.manifest)?;
    writeln!(&mut fixed)?;
    write_saved_clock(&mut fixed, snapshot.saved_clock)?;
    writeln!(&mut fixed)?;
    if fixed.0 > crate::budget::FIXED_SNAPSHOT_HEADER_RESERVE_BYTES {
        return Err(crate::snapshot_limit_exceeded());
    }
    Ok(())
}

fn add_world_metadata(
    per_world: &mut HashMap<minimium_world::world::WorldInstanceId, usize>,
    world: minimium_world::world::WorldInstanceId,
    bytes: usize,
) -> io::Result<()> {
    let total = per_world.get_mut(&world).ok_or_else(|| {
        crate::invalid_world_snapshot("world metadata references an unrecorded instance")
    })?;
    *total = total
        .checked_add(bytes)
        .ok_or_else(crate::snapshot_limit_exceeded)?;
    Ok(())
}

fn encode_player_record(
    writer: &mut impl io::Write,
    id: minimium_core::ids::PlayerId,
    profile_id: minimium_core::identity::PlayerProfileId,
    player: StoredPlayer,
) -> io::Result<()> {
    write!(
        writer,
        "PLAYER\t{}\t{}\t{}",
        id,
        profile_id.to_hex(),
        player.name.as_str()
    )?;
    write_location(writer, player.location)?;
    write!(writer, "\t{}", player.health)?;
    write_location(writer, player.return_location)?;
    writeln!(writer)
}
