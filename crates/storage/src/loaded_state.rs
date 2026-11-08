use super::{player_store::PlayerStore, PersistedEntity};
use minimium_environment::time::SavedClockState;

pub struct LoadedState {
    pub player_store: PlayerStore,
    pub saved_clock: Option<SavedClockState>,
    pub source_version: Option<u8>,
    pub entity_counters: Vec<(minimium_world::world::WorldInstanceId, u64)>,
    pub world_initialization: Vec<(minimium_world::world::WorldInstanceId, bool)>,
    pub entities: Vec<PersistedEntity>,
}

impl LoadedState {
    pub fn empty() -> Self {
        Self {
            player_store: PlayerStore::default(),
            saved_clock: None,
            source_version: None,
            entity_counters: Vec::new(),
            world_initialization: Vec::new(),
            entities: Vec::new(),
        }
    }

    pub fn from_snapshot(
        player_store: PlayerStore,
        saved_clock: Option<SavedClockState>,
        source_version: u8,
        entity_counters: Vec<(minimium_world::world::WorldInstanceId, u64)>,
        world_initialization: Vec<(minimium_world::world::WorldInstanceId, bool)>,
        entities: Vec<PersistedEntity>,
    ) -> Self {
        Self {
            player_store,
            saved_clock,
            source_version: Some(source_version),
            entity_counters,
            world_initialization,
            entities,
        }
    }
}
