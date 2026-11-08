mod budget;
mod error;
mod format;
mod loaded_state;
mod persistence;
mod player_store;
pub mod world_location;

use minimium_simulation::Player;
use minimium_world::{
    entity::{ComponentTypeId, EntityId, SchemaVersion},
    world::{CharacterGravity, WorldInstanceId, WorldManifest, WorldSectorState},
};

pub struct WorldSaveInput {
    pub primary_instance: WorldInstanceId,
    pub primary_map_fingerprint: u64,
    pub primary_manifest: WorldManifest,
    pub worlds: Vec<WorldSaveEntry>,
    pub sector_states: Vec<(WorldInstanceId, WorldSectorState)>,
    pub entity_counters: Vec<(WorldInstanceId, u64)>,
    pub world_initialization: Vec<(WorldInstanceId, bool)>,
    pub entities: Vec<PersistedEntity>,
}

#[derive(Clone)]
pub struct PersistedEntity {
    pub world: WorldInstanceId,
    pub id: EntityId,
    pub components: Vec<PersistedComponent>,
}

#[derive(Clone)]
pub struct PersistedComponent {
    pub kind: ComponentTypeId,
    pub schema: SchemaVersion,
    pub bytes: Vec<u8>,
}

pub struct WorldSaveEntry {
    pub instance: WorldInstanceId,
    pub map_fingerprint: u64,
    pub manifest: WorldManifest,
    pub gravity: CharacterGravity,
    pub players: Vec<Player>,
}

pub use budget::{SnapshotBudget, SnapshotRecordKey, MAX_SNAPSHOT_WORLDS};
pub use error::{StorageError, StorageErrorKind};
pub use format::SAVE_VERSION;
pub use loaded_state::LoadedState;
pub use persistence::PersistenceWorker;
pub use player_store::{PlayerStore, SaveSnapshot};

pub fn saved_sector_record_bytes(
    instance: minimium_world::world::WorldInstanceId,
    state: &minimium_world::world::WorldSectorState,
) -> std::io::Result<usize> {
    format::sector_record_bytes(instance, state)
}

pub fn entity_record_bytes(
    instance: minimium_world::world::WorldInstanceId,
    entity: minimium_world::entity::EntityId,
) -> std::io::Result<usize> {
    format::entity_record_bytes(instance, entity)
}

pub fn component_record_bytes(
    instance: minimium_world::world::WorldInstanceId,
    entity: minimium_world::entity::EntityId,
    kind: minimium_world::entity::ComponentTypeId,
    blob: &minimium_world::entity::ComponentBlob,
) -> std::io::Result<usize> {
    format::component_record_bytes(instance, entity, kind, blob)
}

pub fn compact_error(error: &impl std::fmt::Display) -> String {
    error
        .to_string()
        .chars()
        .take(120)
        .map(|character| {
            if character.is_ascii_whitespace() {
                '_'
            } else {
                character
            }
        })
        .collect()
}

pub(crate) fn valid_player_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= minimium_core::identity::MAX_PLAYER_NAME_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

pub(crate) fn invalid_world_snapshot(message: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message)
}

pub(crate) fn snapshot_limit_exceeded() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::FileTooLarge,
        "snapshot exceeds size limit",
    )
}
