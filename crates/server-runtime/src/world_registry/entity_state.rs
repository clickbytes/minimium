use super::*;
use minimium_world::entity::{ComponentBlob, ComponentRegistry, EntityStore};
use std::collections::HashSet;

impl WorldRegistry {
    pub(crate) fn initialize_new_content(&mut self) {
        for owner in self.instances.values_mut() {
            owner.content_initialized = true;
        }
        for owner in self.instances.values_mut() {
            owner.initial_entities.clear();
        }
    }

    pub(crate) fn apply_entity_effect_batch(
        &mut self,
        effects: &[crate::modules::WorldEffect],
        budget: &mut crate::storage::SnapshotBudget,
        registry: &ComponentRegistry,
    ) -> Result<(), RegistryError> {
        if effects.is_empty() {
            return Ok(());
        }
        let mut stores: BTreeMap<_, _> = self
            .instances
            .iter()
            .map(|(instance, owner)| (*instance, owner.entities.clone()))
            .collect();
        let mut next_budget = budget.clone();
        for effect in effects {
            match effect {
                crate::modules::WorldEffect::SpawnEntity {
                    world,
                    sector,
                    components,
                } => {
                    let total: usize = stores.values().map(EntityStore::len).sum();
                    if total >= minimium_world::entity::MAX_PERSISTENT_ENTITIES {
                        return Err(RegistryError::Component(ComponentError::CapacityExceeded));
                    }
                    let manifest = self
                        .instances
                        .get(world)
                        .ok_or(RegistryError::UnknownInstance)?
                        .manifest;
                    if !matches!(
                        (manifest.topology(), sector),
                        (
                            minimium_world::world::WorldTopology::Planet,
                            SectorKey::Planet(_)
                        ) | (
                            minimium_world::world::WorldTopology::Dungeon,
                            SectorKey::Dungeon(_)
                        )
                    ) {
                        return Err(RegistryError::InvalidEntityState);
                    }
                    let store = stores
                        .get_mut(world)
                        .ok_or(RegistryError::UnknownInstance)?;
                    let entity = store.spawn().map_err(RegistryError::Component)?;
                    next_budget
                        .reserve_many([(
                            crate::storage::SnapshotRecordKey::Entity(*world, entity),
                            minimium_storage::entity_record_bytes(*world, entity)
                                .map_err(|_| RegistryError::SnapshotBudgetExceeded)?,
                        )])
                        .map_err(|_| RegistryError::SnapshotBudgetExceeded)?;
                    let mut kinds = HashSet::new();
                    for component in components {
                        if !kinds.insert(component.kind) {
                            return Err(RegistryError::Component(ComponentError::InvalidPayload));
                        }
                        store
                            .insert(
                                registry,
                                entity,
                                component.kind,
                                component.schema,
                                &component.bytes,
                            )
                            .map_err(RegistryError::Component)?;
                        let blob = ComponentBlob::new(component.schema, component.bytes.clone())
                            .ok_or(RegistryError::Component(ComponentError::InvalidPayload))?;
                        let bytes = minimium_storage::component_record_bytes(
                            *world,
                            entity,
                            component.kind,
                            &blob,
                        )
                        .map_err(|_| RegistryError::SnapshotBudgetExceeded)?;
                        let key = crate::storage::SnapshotRecordKey::Component(
                            *world,
                            entity,
                            component.kind,
                        );
                        next_budget
                            .reserve_many([(key, bytes)])
                            .map_err(|_| RegistryError::SnapshotBudgetExceeded)?;
                        next_budget
                            .reserve_component_payloads([(key, component.bytes.len())])
                            .map_err(|_| RegistryError::SnapshotBudgetExceeded)?;
                    }
                }
                crate::modules::WorldEffect::DespawnEntity { world, entity } => {
                    let store = stores
                        .get_mut(world)
                        .ok_or(RegistryError::UnknownInstance)?;
                    let kinds: Vec<_> = store
                        .iter_components()
                        .filter(|(id, _, _)| id == entity)
                        .map(|(_, kind, _)| kind)
                        .collect();
                    if !store.despawn(*entity).map_err(RegistryError::Component)? {
                        return Err(RegistryError::EntitySectorConflict);
                    }
                    next_budget.release(crate::storage::SnapshotRecordKey::Entity(*world, *entity));
                    for kind in kinds {
                        next_budget.release(crate::storage::SnapshotRecordKey::Component(
                            *world, *entity, kind,
                        ));
                    }
                }
                crate::modules::WorldEffect::SetComponent {
                    world,
                    entity,
                    component,
                } => {
                    let store = stores
                        .get_mut(world)
                        .ok_or(RegistryError::UnknownInstance)?;
                    store
                        .insert(
                            registry,
                            *entity,
                            component.kind,
                            component.schema,
                            &component.bytes,
                        )
                        .map_err(RegistryError::Component)?;
                    let blob = ComponentBlob::new(component.schema, component.bytes.clone())
                        .ok_or(RegistryError::Component(ComponentError::InvalidPayload))?;
                    let bytes = minimium_storage::component_record_bytes(
                        *world,
                        *entity,
                        component.kind,
                        &blob,
                    )
                    .map_err(|_| RegistryError::SnapshotBudgetExceeded)?;
                    let key = crate::storage::SnapshotRecordKey::Component(
                        *world,
                        *entity,
                        component.kind,
                    );
                    next_budget
                        .reserve(key, bytes)
                        .map_err(|_| RegistryError::SnapshotBudgetExceeded)?;
                    next_budget
                        .reserve_component_payloads([(key, component.bytes.len())])
                        .map_err(|_| RegistryError::SnapshotBudgetExceeded)?;
                }
                crate::modules::WorldEffect::RemoveComponent {
                    world,
                    entity,
                    kind,
                } => {
                    let store = stores
                        .get_mut(world)
                        .ok_or(RegistryError::UnknownInstance)?;
                    if store
                        .remove_component(*entity, *kind)
                        .map_err(RegistryError::Component)?
                    {
                        next_budget.release(crate::storage::SnapshotRecordKey::Component(
                            *world, *entity, *kind,
                        ));
                    }
                }
                crate::modules::WorldEffect::RequestTransfer { .. } => {}
            }
        }
        for (instance, store) in stores {
            self.instances
                .get_mut(&instance)
                .expect("staged world remains registered")
                .entities = store;
        }
        *budget = next_budget;
        Ok(())
    }

    pub(crate) fn restore_persistent_entities(
        &mut self,
        counters: &[(WorldInstanceId, u64)],
        initialized: &[(WorldInstanceId, bool)],
        records: &[crate::storage::PersistedEntity],
        registry: &ComponentRegistry,
    ) -> Result<(), RegistryError> {
        if counters.is_empty() && initialized.is_empty() && records.is_empty() {
            return Ok(());
        }
        if counters.len() != self.instances.len() || initialized.len() != self.instances.len() {
            return Err(RegistryError::InvalidEntityState);
        }
        let mut staged: BTreeMap<_, _> = self
            .instances
            .iter()
            .map(|(instance, owner)| (*instance, owner.entities.clone()))
            .collect();
        for record in records {
            let store = staged
                .get_mut(&record.world)
                .ok_or(RegistryError::UnknownInstance)?;
            store.restore(record.id).map_err(RegistryError::Component)?;
            for component in &record.components {
                store
                    .restore_component(
                        registry,
                        record.id,
                        component.kind,
                        component.schema,
                        &component.bytes,
                    )
                    .map_err(RegistryError::Component)?;
            }
        }
        for (instance, next_id) in counters {
            staged
                .get_mut(instance)
                .ok_or(RegistryError::UnknownInstance)?
                .restore_counter(*next_id)
                .map_err(RegistryError::Component)?;
        }
        for (instance, _) in initialized {
            if !self.instances.contains_key(instance) {
                return Err(RegistryError::UnknownInstance);
            }
        }
        for (instance, value) in initialized {
            self.instances
                .get_mut(instance)
                .expect("validated world exists")
                .content_initialized = *value;
        }
        for (instance, store) in staged {
            self.instances
                .get_mut(&instance)
                .expect("staged world remains registered")
                .entities = store;
        }
        Ok(())
    }
}
