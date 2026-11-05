mod format;
mod loaded_state;
mod persistence;
mod player_store;
pub mod world_location;

use minimium_simulation::Player;
use minimium_world::world::{CharacterGravity, WorldInstanceId, WorldManifest, WorldSectorState};

pub struct WorldSaveInput {
    pub primary_instance: WorldInstanceId,
    pub primary_map_fingerprint: u64,
    pub primary_manifest: WorldManifest,
    pub worlds: Vec<WorldSaveEntry>,
    pub sector_states: Vec<(WorldInstanceId, WorldSectorState)>,
}

pub struct WorldSaveEntry {
    pub instance: WorldInstanceId,
    pub map_fingerprint: u64,
    pub manifest: WorldManifest,
    pub gravity: CharacterGravity,
    pub players: Vec<Player>,
}

pub use format::SAVE_VERSION;
pub use loaded_state::LoadedState;
pub use persistence::PersistenceWorker;
pub use player_store::{PlayerStore, SaveSnapshot};

pub fn compact_error(error: &std::io::Error) -> String {
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
