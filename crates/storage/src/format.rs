use super::{
    invalid_world_snapshot,
    player_store::{PlayerStore, SaveSnapshot, SavedWorldManifest, StoredName, StoredPlayer},
    LoadedState,
};
use minimium_core::{identity::PlayerProfileId, ids::PlayerId};
use minimium_simulation::PLAYER_MAX_HEALTH;
use minimium_world::world::{
    GenerationRevision, GenerationSpec, GeneratorDigest, PlanetDefinition, Position3Mm, SectorKey,
    WorldContentRevision, WorldDefinitionId, WorldInstanceId, WorldLocation, WorldManifest,
    WorldMap, WorldOrientationQ30, WorldSectorState, WorldSeed, WorldTopology,
    MAX_PERSISTED_WORLD_OBJECT_STATES, MAX_PERSISTED_WORLD_OBJECT_STATE_BYTES,
    MAX_PERSISTED_WORLD_SECTORS,
};
use std::{
    collections::HashSet,
    io::{self, Write},
    path::Path,
    sync::Arc,
};

pub const SAVE_VERSION: u8 = 10;
pub(super) const MAX_STORED_PLAYERS: usize = 100_000;
pub(super) const MAX_WORLD_SNAPSHOT_BYTES: usize = 128 * 1024 * 1024;
pub(super) const MAX_SAVED_POSITION_MM: u64 = 7_000_000_000;
pub(super) const PLAYER_STORE_SHARDS: usize = 256;

mod clock;
mod load;
mod locations;
mod sectors;
mod worlds;
mod write;

use clock::{parse_saved_clock, write_saved_clock};
use locations::{parse_location, position_from_location, write_location};
use sectors::{parse_saved_sector, write_saved_sector};
pub(crate) use worlds::parse_manifest;
use worlds::{parse_saved_world, validate_saved_world, write_manifest, write_saved_world};

pub(super) fn load(
    data_dir: &Path,
    worlds: &[(
        WorldInstanceId,
        minimium_world::world::WorldMap,
        minimium_world::world::WorldManifest,
    )],
    components: &minimium_world::entity::ComponentRegistry,
) -> io::Result<LoadedState> {
    load::load(data_dir, worlds, components)
}

pub(super) fn write(snapshot: SaveSnapshot, data_dir: &Path) -> io::Result<()> {
    write::write(snapshot, data_dir)
}

pub(crate) fn maximum_profile_record_bytes() -> io::Result<usize> {
    write::maximum_profile_record_bytes()
}

pub(crate) fn sector_record_bytes(
    instance: WorldInstanceId,
    state: &WorldSectorState,
) -> io::Result<usize> {
    write::sector_record_bytes(instance, state)
}

pub(crate) fn entity_record_bytes(
    instance: minimium_world::world::WorldInstanceId,
    entity: minimium_world::entity::EntityId,
) -> io::Result<usize> {
    write::entity_record_bytes(instance, entity)
}

pub(crate) fn component_record_bytes(
    instance: minimium_world::world::WorldInstanceId,
    entity: minimium_world::entity::EntityId,
    kind: minimium_world::entity::ComponentTypeId,
    blob: &minimium_world::entity::ComponentBlob,
) -> io::Result<usize> {
    write::component_record_bytes(instance, entity, kind, blob)
}
fn hex_digest(bytes: &[u8; 32]) -> String {
    let mut result = String::with_capacity(64);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(result, "{byte:02x}");
    }
    result
}
