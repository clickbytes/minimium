use super::player_store::PlayerStore;
use minimium_environment::time::SavedClockState;

pub struct LoadedState {
    pub player_store: PlayerStore,
    pub saved_clock: Option<SavedClockState>,
    pub source_version: Option<u8>,
}

impl LoadedState {
    pub fn empty() -> Self {
        Self {
            player_store: PlayerStore::default(),
            saved_clock: None,
            source_version: None,
        }
    }

    pub fn from_snapshot(
        player_store: PlayerStore,
        saved_clock: Option<SavedClockState>,
        source_version: u8,
    ) -> Self {
        Self {
            player_store,
            saved_clock,
            source_version: Some(source_version),
        }
    }
}
