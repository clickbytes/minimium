use super::*;
use minimium_world::world::{
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
    #[cfg(test)]
    pub(crate) fn commit_sector_state(
        &mut self,
        instance: WorldInstanceId,
        state: WorldSectorState,
        budget: &mut crate::storage::SnapshotBudget,
    ) -> Result<(), RegistryError> {
        let (address, totals) = self.validate_sector_replacement(
            &self.sector_states,
            self.persisted_totals.objects,
            self.persisted_totals.bytes,
            instance,
            &state,
        )?;
        let mut next_budget = budget.clone();
        let bytes = minimium_storage::saved_sector_record_bytes(instance, &state)
            .map_err(|_| RegistryError::SnapshotBudgetExceeded)?;
        next_budget
            .reserve(
                crate::storage::SnapshotRecordKey::Sector(instance, state.key()),
                bytes,
            )
            .map_err(|_| RegistryError::SnapshotBudgetExceeded)?;
        self.sector_states.insert(address, state);
        self.persisted_totals = totals;
        *budget = next_budget;
        Ok(())
    }

    pub(crate) fn restore_sector_states(
        &mut self,
        states: impl IntoIterator<Item = (WorldInstanceId, WorldSectorState)>,
        budget: &mut crate::storage::SnapshotBudget,
    ) -> Result<(), RegistryError> {
        let mut candidate = self.sector_states.clone();
        let mut totals = PersistedTotals {
            objects: self.persisted_totals.objects,
            bytes: self.persisted_totals.bytes,
        };
        let mut next_budget = budget.clone();
        for (instance, state) in states {
            let (address, next_totals) = self.validate_sector_replacement(
                &candidate,
                totals.objects,
                totals.bytes,
                instance,
                &state,
            )?;
            let bytes = minimium_storage::saved_sector_record_bytes(instance, &state)
                .map_err(|_| RegistryError::SnapshotBudgetExceeded)?;
            next_budget
                .reserve(
                    crate::storage::SnapshotRecordKey::Sector(instance, state.key()),
                    bytes,
                )
                .map_err(|_| RegistryError::SnapshotBudgetExceeded)?;
            candidate.insert(address, state);
            totals = next_totals;
        }
        self.sector_states = candidate;
        self.persisted_totals = totals;
        *budget = next_budget;
        Ok(())
    }

    fn validate_sector_replacement(
        &self,
        sectors: &BTreeMap<(WorldInstanceId, minimium_world::world::SectorKey), WorldSectorState>,
        current_objects: usize,
        current_bytes: usize,
        instance: WorldInstanceId,
        state: &WorldSectorState,
    ) -> Result<
        (
            (WorldInstanceId, minimium_world::world::SectorKey),
            PersistedTotals,
        ),
        RegistryError,
    > {
        let owner = self
            .instances
            .get(&instance)
            .ok_or(RegistryError::UnknownInstance)?;
        let key_matches = matches!(
            (owner.manifest.topology(), state.key()),
            (
                WorldTopology::Planet,
                minimium_world::world::SectorKey::Planet(_)
            ) | (
                WorldTopology::Dungeon,
                minimium_world::world::SectorKey::Dungeon(_)
            )
        );
        if !key_matches {
            return Err(RegistryError::InvalidSectorState);
        }
        let address = (instance, state.key());
        if sectors
            .get(&address)
            .is_some_and(|previous| previous.revision() >= state.revision())
        {
            return Err(RegistryError::StaleSectorRevision);
        }
        if !sectors.contains_key(&address) && sectors.len() >= MAX_PERSISTED_WORLD_SECTORS {
            return Err(RegistryError::SectorStateCapacity);
        }
        let replaced = sectors.get(&address);
        let old_objects = replaced.map_or(0, |sector| sector.objects().len());
        let old_bytes = replaced
            .into_iter()
            .flat_map(|sector| sector.objects())
            .map(|object| object.data().len())
            .try_fold(0usize, usize::checked_add)
            .ok_or(RegistryError::SectorStateCapacity)?;
        let new_objects = current_objects
            .checked_sub(old_objects)
            .and_then(|value| value.checked_add(state.objects().len()))
            .ok_or(RegistryError::SectorStateCapacity)?;
        let new_bytes = current_bytes
            .checked_sub(old_bytes)
            .and_then(|value| {
                state
                    .objects()
                    .iter()
                    .map(|object| object.data().len())
                    .try_fold(value, usize::checked_add)
            })
            .ok_or(RegistryError::SectorStateCapacity)?;
        if new_objects > MAX_PERSISTED_WORLD_OBJECT_STATES
            || new_bytes > MAX_PERSISTED_WORLD_OBJECT_STATE_BYTES
        {
            return Err(RegistryError::SectorStateCapacity);
        }
        Ok((
            address,
            PersistedTotals {
                objects: new_objects,
                bytes: new_bytes,
            },
        ))
    }

    pub(crate) fn sector_states(&self) -> Vec<(WorldInstanceId, WorldSectorState)> {
        self.sector_states
            .iter()
            .map(|((instance, _), state)| (*instance, state.clone()))
            .collect()
    }
}
