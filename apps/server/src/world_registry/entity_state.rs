use super::*;
use minimium_core::{
    entity::{ComponentBlob, ComponentRegistry, EntityStore, ReplicationPolicy},
    world::{SectorRevision, WorldObjectState},
};

pub(crate) const MAX_REPLICATION_CANDIDATES: usize = 256;
pub(crate) const MAX_REPLICATION_CANDIDATE_BYTES: usize = 64 * 1024;

pub(crate) type ReplicationCandidate = (EntityId, ComponentTypeId, ComponentBlob);

impl WorldRegistry {
    /// Collects a bounded, read-only staging view of changed replicated components.
    pub(crate) fn replication_candidates(
        &self,
        instance: WorldInstanceId,
        registry: &ComponentRegistry,
    ) -> Result<(Vec<ReplicationCandidate>, usize), RegistryError> {
        let store = &self
            .instances
            .get(&instance)
            .ok_or(RegistryError::UnknownInstance)?
            .entities;
        let mut candidates = Vec::new();
        let mut filtered = 0usize;
        let mut bytes = 0usize;
        for (entity, kind, blob) in store.iter_changed_components() {
            let policy = registry
                .descriptor(kind)
                .map(|descriptor| descriptor.replication)
                .unwrap_or(ReplicationPolicy::ServerOnly);
            let next_bytes = bytes.saturating_add(blob.bytes.len());
            if policy != ReplicationPolicy::Replicated
                || candidates.len() >= MAX_REPLICATION_CANDIDATES
                || next_bytes > MAX_REPLICATION_CANDIDATE_BYTES
            {
                filtered = filtered.saturating_add(1);
                continue;
            }
            bytes = next_bytes;
            candidates.push((entity, kind, blob.clone()));
        }
        Ok((candidates, filtered))
    }
}

impl WorldRegistry {
    /// Creates a component entity and persists its component into the selected sector.
    pub(crate) fn spawn_component_entity(
        &mut self,
        instance: WorldInstanceId,
        sector: SectorKey,
        origin: ObjectOriginId,
        registry: &ComponentRegistry,
        kind: ComponentTypeId,
        blob: &ComponentBlob,
    ) -> Result<EntityId, RegistryError> {
        if !self.instances.contains_key(&instance) {
            return Err(RegistryError::UnknownInstance);
        }
        let store = &self.instances.get(&instance).unwrap().entities;
        let mut candidate = store.clone();
        let entity = candidate.spawn().map_err(RegistryError::Component)?;
        candidate
            .insert(registry, entity, kind, blob.schema, &blob.bytes)
            .map_err(RegistryError::Component)?;
        if self
            .sector_states
            .get(&(instance, sector))
            .is_some_and(|state| {
                state
                    .objects()
                    .iter()
                    .any(|object| object.origin() == origin)
            })
        {
            return Err(RegistryError::EntityOriginConflict);
        }
        let revision = self.next_sector_revision(instance, sector)?;
        self.entity_sectors.insert((instance, entity), sector);
        self.entity_origins.insert((instance, entity, kind), origin);
        let objects = match self.objects_with_component_changes(instance, sector, &candidate, None)
        {
            Ok(objects) => objects,
            Err(error) => {
                self.entity_sectors.remove(&(instance, entity));
                self.entity_origins.remove(&(instance, entity, kind));
                return Err(error);
            }
        };
        let Some(state) = WorldSectorState::new(sector, revision, objects) else {
            self.entity_sectors.remove(&(instance, entity));
            self.entity_origins.remove(&(instance, entity, kind));
            return Err(RegistryError::SectorStateCapacity);
        };
        if let Err(error) = self.commit_sector_state(instance, state) {
            self.entity_sectors.remove(&(instance, entity));
            self.entity_origins.remove(&(instance, entity, kind));
            return Err(error);
        }
        self.instances.get_mut(&instance).unwrap().entities = candidate;
        Ok(entity)
    }

    pub(crate) fn despawn_component_entity(
        &mut self,
        instance: WorldInstanceId,
        entity: EntityId,
        registry: &ComponentRegistry,
    ) -> Result<(), RegistryError> {
        let sector = *self
            .entity_sectors
            .get(&(instance, entity))
            .ok_or(RegistryError::EntitySectorConflict)?;
        let mut candidate = self
            .instances
            .get(&instance)
            .ok_or(RegistryError::UnknownInstance)?
            .entities
            .clone();
        if !candidate.despawn(entity) {
            return Err(RegistryError::EntitySectorConflict);
        }
        let origins: Vec<_> = self
            .entity_origins
            .keys()
            .filter(|(owner, id, _)| *owner == instance && *id == entity)
            .copied()
            .collect();
        let objects =
            self.objects_with_component_changes(instance, sector, &candidate, Some(entity))?;
        self.validate_component_objects(&objects, registry)?;
        let revision = self.next_sector_revision(instance, sector)?;
        let state = WorldSectorState::new(sector, revision, objects)
            .ok_or(RegistryError::SectorStateCapacity)?;
        self.commit_sector_state(instance, state)?;
        self.instances.get_mut(&instance).unwrap().entities = candidate;
        self.entity_sectors.remove(&(instance, entity));
        for key in origins {
            self.entity_origins.remove(&key);
        }
        Ok(())
    }

    /// Persists the current component store as a complete replacement of one sector.
    pub(crate) fn commit_entity_sector(
        &mut self,
        instance: WorldInstanceId,
        sector: SectorKey,
        registry: &ComponentRegistry,
    ) -> Result<(), RegistryError> {
        let store = &self
            .instances
            .get(&instance)
            .ok_or(RegistryError::UnknownInstance)?
            .entities;
        let objects = self.objects_with_component_changes(instance, sector, store, None)?;
        self.validate_component_objects(&objects, registry)?;
        let revision = self.next_sector_revision(instance, sector)?;
        let state = WorldSectorState::new(sector, revision, objects)
            .ok_or(RegistryError::SectorStateCapacity)?;
        self.commit_sector_state(instance, state)
    }

    /// Rehydrates MEC1 objects while leaving legacy objects untouched.
    pub(crate) fn rehydrate_component_objects(
        &mut self,
        registry: &ComponentRegistry,
    ) -> Result<(), RegistryError> {
        let mut stores: BTreeMap<WorldInstanceId, EntityStore> = self
            .instances
            .iter()
            .map(|(id, owner)| (*id, owner.entities.clone()))
            .collect();
        let mut sectors = BTreeMap::new();
        let mut origins = BTreeMap::new();
        for ((instance, sector), state) in &self.sector_states {
            for object in state.objects() {
                let Some((entity, kind, blob)) = EntityStore::decode_world_object(registry, object)
                    .map_err(RegistryError::Component)?
                else {
                    continue;
                };
                if sectors
                    .insert((*instance, entity), *sector)
                    .is_some_and(|previous| previous != *sector)
                {
                    return Err(RegistryError::EntitySectorConflict);
                }
                if origins
                    .insert((*instance, entity, kind), object.origin())
                    .is_some()
                {
                    return Err(RegistryError::EntityOriginConflict);
                }
                stores
                    .get_mut(instance)
                    .ok_or(RegistryError::UnknownInstance)?
                    .restore_component(registry, entity, kind, blob.schema, &blob.bytes)
                    .map_err(RegistryError::Component)?;
            }
        }
        for (id, store) in stores {
            self.instances.get_mut(&id).unwrap().entities = store;
        }
        self.entity_sectors = sectors;
        self.entity_origins = origins;
        Ok(())
    }

    fn next_sector_revision(
        &self,
        instance: WorldInstanceId,
        sector: SectorKey,
    ) -> Result<SectorRevision, RegistryError> {
        let next = self
            .sector_states
            .get(&(instance, sector))
            .map_or(Some(1), |state| state.revision().get().checked_add(1))
            .ok_or(RegistryError::SectorRevisionExhausted)?;
        SectorRevision::new(next).ok_or(RegistryError::SectorRevisionExhausted)
    }

    fn objects_with_component_changes(
        &self,
        instance: WorldInstanceId,
        sector: SectorKey,
        store: &EntityStore,
        removed_entity: Option<EntityId>,
    ) -> Result<Vec<WorldObjectState>, RegistryError> {
        let mut objects: Vec<_> = self
            .sector_states
            .get(&(instance, sector))
            .into_iter()
            .flat_map(|state| state.objects())
            .filter(|object| {
                !self
                    .entity_origins
                    .iter()
                    .any(|((owner, entity, _), origin)| {
                        *owner == instance
                            && self.entity_sectors.get(&(*owner, *entity)) == Some(&sector)
                            && *origin == object.origin()
                            && Some(*entity) != removed_entity
                    })
            })
            .cloned()
            .collect();
        for (entity, kind, _) in store.iter_components() {
            if self.entity_sectors.get(&(instance, entity)) != Some(&sector)
                && Some(entity) != removed_entity
            {
                continue;
            }
            let origin = self
                .entity_origins
                .get(&(instance, entity, kind))
                .copied()
                .ok_or(RegistryError::EntityOriginConflict)?;
            objects.push(
                store
                    .to_world_object(origin, entity, kind)
                    .ok_or(RegistryError::SectorStateCapacity)?,
            );
        }
        Ok(objects)
    }

    fn validate_component_objects(
        &self,
        objects: &[WorldObjectState],
        registry: &ComponentRegistry,
    ) -> Result<(), RegistryError> {
        for object in objects {
            EntityStore::decode_world_object(registry, object).map_err(RegistryError::Component)?;
        }
        Ok(())
    }
}
