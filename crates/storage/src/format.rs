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
    WorldMap, WorldOrientationQ30, WorldSeed, WorldTopology, MAX_PERSISTED_WORLD_OBJECT_STATES,
    MAX_PERSISTED_WORLD_OBJECT_STATE_BYTES, MAX_PERSISTED_WORLD_SECTORS,
};
use std::{
    collections::HashSet,
    io::{self, Write},
    path::Path,
    sync::Arc,
};

pub const SAVE_VERSION: u8 = 9;
pub(super) const MAX_STORED_PLAYERS: usize = 100_000;
pub(super) const MAX_WORLD_SNAPSHOT_BYTES: usize = 16 * 1024 * 1024;
pub(super) const MAX_SAVED_COORDINATE: f32 = 7_000_000.0;
pub(super) const MAX_SAVED_POSITION_MM: u64 = 7_000_000_000;
pub(super) const PLAYER_STORE_SHARDS: usize = 256;

mod clock;
mod load;
mod locations;
mod sectors;
mod worlds;
mod write;

use clock::{parse_saved_clock, write_saved_clock};
use locations::{legacy_location, parse_location, position_from_location, write_location};
use sectors::{parse_saved_sector, write_saved_sector};
use worlds::{parse_saved_world, validate_saved_world, write_manifest, write_saved_world};

pub(super) fn load(
    data_dir: &Path,
    worlds: &[(
        WorldInstanceId,
        minimium_world::world::WorldMap,
        minimium_world::world::WorldManifest,
    )],
) -> io::Result<LoadedState> {
    load::load(data_dir, worlds)
}

pub(super) fn write(snapshot: SaveSnapshot, data_dir: &Path) -> io::Result<()> {
    write::write(snapshot, data_dir)
}
fn hex_digest(bytes: &[u8; 32]) -> String {
    let mut result = String::with_capacity(64);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(result, "{byte:02x}");
    }
    result
}
