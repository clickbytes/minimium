use super::{PlayerStore, SaveSnapshot, SavedWorldManifest};
use crate::StorageError;
use crate::WorldSaveInput;
use minimium_environment::time::SavedClockState;
use std::{io, path::Path};

pub(super) fn capture(
    store: &mut PlayerStore,
    input: WorldSaveInput,
    saved_clock: SavedClockState,
    revision: u64,
) -> io::Result<SaveSnapshot> {
    let _storage_phase =
        minimium_telemetry::allocations::enter(minimium_telemetry::allocations::Phase::Storage);
    for world in &input.worlds {
        for player in &world.players {
            store.update_from_player(player, world.instance, world.gravity)?;
        }
    }
    let player_shards = store.players.iter().map(std::sync::Arc::clone).collect();
    let world_manifests = input
        .worlds
        .into_iter()
        .map(|world| SavedWorldManifest {
            instance: world.instance,
            map_fingerprint: world.map_fingerprint,
            manifest: world.manifest,
        })
        .collect();
    let mut entity_counters = input.entity_counters;
    let mut entities = input.entities;
    let mut world_initialization = input.world_initialization;
    entity_counters.sort_unstable_by_key(|(world, _)| *world);
    world_initialization.sort_unstable_by_key(|(world, _)| *world);
    entities.sort_unstable_by_key(|entity| (entity.world, entity.id));
    Ok(SaveSnapshot {
        revision,
        saved_clock,
        primary_instance: input.primary_instance,
        map_fingerprint: input.primary_map_fingerprint,
        manifest: input.primary_manifest,
        world_manifests,
        sector_states: input.sector_states,
        entity_counters,
        world_initialization,
        entities,
        player_shards,
    })
}

impl SaveSnapshot {
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn write(self, data_dir: &Path) -> Result<(), StorageError> {
        crate::format::write(self, data_dir).map_err(StorageError::from_io)
    }
}
