use super::player_store::PlayerStore;
use minimium_core::time::SavedClockState;

pub(crate) struct LoadedState {
    pub(crate) player_store: PlayerStore,
    pub(crate) saved_clock: Option<SavedClockState>,
    pub(crate) source_version: Option<u8>,
}

impl LoadedState {
    pub(crate) fn empty() -> Self {
        Self {
            player_store: PlayerStore::default(),
            saved_clock: None,
            source_version: None,
        }
    }

    pub(crate) fn from_snapshot(
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
