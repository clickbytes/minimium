use super::*;
use minimium_core::world::{
    MAX_PERSISTED_WORLD_OBJECT_STATES, MAX_PERSISTED_WORLD_OBJECT_STATE_BYTES,
    MAX_PERSISTED_WORLD_SECTORS,
};

#[derive(Default)]
pub(super) struct PersistedTotals {
    objects: usize,
    bytes: usize,
}

impl WorldRegistry {
    /// Commits one complete logical sector state on the registry's owner lane.
    /// A higher revision replaces the prior object set; omitted IDs are deleted.
    pub(crate) fn commit_sector_state(
        &mut self,
        instance: WorldInstanceId,
        state: WorldSectorState,
    ) -> Result<(), RegistryError> {
        let owner = self
            .instances
            .get(&instance)
            .ok_or(RegistryError::UnknownInstance)?;
        let key_matches = matches!(
            (owner.manifest.topology(), state.key()),
            (
                WorldTopology::Planet,
                minimium_core::world::SectorKey::Planet(_)
            ) | (
                WorldTopology::Dungeon,
                minimium_core::world::SectorKey::Dungeon(_)
            )
        );
        if !key_matches {
            return Err(RegistryError::InvalidSectorState);
        }
        let address = (instance, state.key());
        if self
            .sector_states
            .get(&address)
            .is_some_and(|previous| previous.revision() >= state.revision())
        {
            return Err(RegistryError::StaleSectorRevision);
        }
        if !self.sector_states.contains_key(&address)
            && self.sector_states.len() >= MAX_PERSISTED_WORLD_SECTORS
        {
            return Err(RegistryError::SectorStateCapacity);
        }
        let replaced = self.sector_states.get(&address);
        let replaced_objects = replaced.map_or(0, |sector| sector.objects().len());
        let replaced_bytes = replaced
            .into_iter()
            .flat_map(|sector| sector.objects())
            .map(|object| object.data().len())
            .try_fold(0usize, usize::checked_add)
            .ok_or(RegistryError::SectorStateCapacity)?;
        let new_objects = state.objects().len();
        let new_bytes = state
            .objects()
            .iter()
            .map(|object| object.data().len())
            .try_fold(0usize, usize::checked_add)
            .ok_or(RegistryError::SectorStateCapacity)?;
        let next_objects = self
            .persisted_totals
            .objects
            .checked_sub(replaced_objects)
            .and_then(|total| total.checked_add(new_objects))
            .ok_or(RegistryError::SectorStateCapacity)?;
        let next_bytes = self
            .persisted_totals
            .bytes
            .checked_sub(replaced_bytes)
            .and_then(|total| total.checked_add(new_bytes))
            .ok_or(RegistryError::SectorStateCapacity)?;
        if next_objects > MAX_PERSISTED_WORLD_OBJECT_STATES
            || next_bytes > MAX_PERSISTED_WORLD_OBJECT_STATE_BYTES
        {
            return Err(RegistryError::SectorStateCapacity);
        }
        self.sector_states.insert(address, state);
        self.persisted_totals = PersistedTotals {
            objects: next_objects,
            bytes: next_bytes,
        };
        Ok(())
    }

    pub(crate) fn restore_sector_states(
        &mut self,
        states: impl IntoIterator<Item = (WorldInstanceId, WorldSectorState)>,
    ) -> Result<(), RegistryError> {
        for (instance, state) in states {
            self.commit_sector_state(instance, state)?;
        }
        Ok(())
    }

    pub(crate) fn sector_states(&self) -> Vec<(WorldInstanceId, WorldSectorState)> {
        self.sector_states
            .iter()
            .map(|((instance, _), state)| (*instance, state.clone()))
            .collect()
    }
}
