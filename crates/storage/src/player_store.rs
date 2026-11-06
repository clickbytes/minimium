mod migration;
mod profiles;
mod snapshot;

use super::{format, LoadedState};
use minimium_core::identity::PlayerProfileId;
use minimium_core::identity::MAX_PLAYER_NAME_BYTES;
use minimium_core::ids::PlayerId;
use minimium_environment::time::SavedClockState;
use minimium_world::world::{WorldInstanceId, WorldManifest, WorldMap, WorldSectorState};
use std::{
    collections::{HashMap, HashSet},
    io,
    path::Path,
    sync::Arc,
};

#[derive(Clone, Copy)]
pub struct StoredPlayer {
    pub id: PlayerId,
    pub name: StoredName,
    pub position: [f32; 3],
    pub health: u16,
    pub location: minimium_world::world::WorldLocation,
    pub return_location: minimium_world::world::WorldLocation,
}

#[derive(Clone, Copy)]
pub struct StoredName {
    bytes: [u8; MAX_PLAYER_NAME_BYTES],
    length: u8,
}

impl StoredName {
    pub(super) fn new(value: &str) -> Option<Self> {
        if !super::valid_player_name(value) {
            return None;
        }
        let mut bytes = [0; MAX_PLAYER_NAME_BYTES];
        bytes[..value.len()].copy_from_slice(value.as_bytes());
        Some(Self {
            bytes,
            length: value.len() as u8,
        })
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..usize::from(self.length)]).unwrap_or("")
    }
}

pub struct PlayerStore {
    pub(super) players: Vec<Arc<HashMap<PlayerProfileId, StoredPlayer>>>,
    pub(super) profile_by_player: HashMap<PlayerId, PlayerProfileId>,
    active_players: HashSet<PlayerId>,
    pub(super) sector_states: Vec<(WorldInstanceId, WorldSectorState)>,
}

pub struct SaveSnapshot {
    pub(super) saved_clock: SavedClockState,
    pub(super) primary_instance: WorldInstanceId,
    pub(super) map_fingerprint: u64,
    pub(super) manifest: WorldManifest,
    pub(super) world_manifests: Vec<SavedWorldManifest>,
    pub(super) sector_states: Vec<(WorldInstanceId, WorldSectorState)>,
    pub(super) player_shards: Vec<Arc<HashMap<PlayerProfileId, StoredPlayer>>>,
}

pub(super) struct SavedWorldManifest {
    pub(super) instance: WorldInstanceId,
    pub(super) map_fingerprint: u64,
    pub(super) manifest: WorldManifest,
}

impl Default for PlayerStore {
    fn default() -> Self {
        Self {
            players: (0..format::PLAYER_STORE_SHARDS)
                .map(|_| Arc::new(HashMap::new()))
                .collect(),
            profile_by_player: HashMap::new(),
            active_players: HashSet::new(),
            sector_states: Vec::new(),
        }
    }
}

impl PlayerStore {
    pub fn load(
        data_dir: &Path,
        worlds: &[(WorldInstanceId, WorldMap, WorldManifest)],
    ) -> io::Result<LoadedState> {
        format::load(data_dir, worlds)
    }

    pub fn sector_states(&self) -> Vec<(WorldInstanceId, WorldSectorState)> {
        self.sector_states.clone()
    }

    pub fn save_snapshot(
        &mut self,
        input: super::WorldSaveInput,
        saved_clock: SavedClockState,
    ) -> io::Result<SaveSnapshot> {
        snapshot::capture(self, input, saved_clock)
    }
}
