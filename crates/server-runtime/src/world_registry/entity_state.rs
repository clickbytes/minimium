use super::*;
use crate::runtime::{ComponentOwnerAuthority, ComponentOwnerPort};
use minimium_world::entity::{
    ComponentBlob, ComponentMutation, ComponentMutationPolicy, ComponentRegistry, EntityStore,
    WorldEntityChange,
};
use std::collections::HashSet;

pub(crate) struct StagedEntityBatch<'a> {
    worlds: &'a mut WorldRegistry,
    stores: BTreeMap<WorldInstanceId, EntityStore>,
    changes: Vec<WorldEntityChange>,
    spawned_entities: Vec<(WorldInstanceId, minimium_world::entity::EntityId)>,
    next_membership_revision: Option<u128>,
}

impl StagedEntityBatch<'_> {
    pub(crate) fn changes(&self) -> &[WorldEntityChange] {
        &self.changes
    }

    pub(crate) fn spawned_entities(
        &self,
    ) -> &[(WorldInstanceId, minimium_world::entity::EntityId)] {
        &self.spawned_entities
    }

    pub(crate) fn commit(self, reservation: minimium_storage::PreparedEntityReservation<'_>) {
        reservation.commit();
        let next_membership_revision = self.next_membership_revision;
        for (instance, store) in self.stores {
            self.worlds
                .instances
                .get_mut(&instance)
                .expect("staged world remains registered while entity batch holds its borrow")
                .entities = store;
        }
        if let Some(revision) = next_membership_revision {
            self.worlds.entity_membership_revision = revision;
        }
    }
}

impl WorldRegistry {
    pub(crate) fn initialize_new_content(&mut self) {
        for owner in self.instances.values_mut() {
            owner.content_initialized = true;
        }
        for owner in self.instances.values_mut() {
            owner.initial_entities.clear();
        }
    }

    pub(crate) fn stage_entity_effect_batch<'a>(
        &'a mut self,
        effects: &[crate::modules::WorldEffect],
        registry: &ComponentRegistry,
        reservations: &ComponentOwnerAuthority,
        authorization: Option<&ComponentOwnerPort>,
    ) -> Result<StagedEntityBatch<'a>, RegistryError> {
        let mut stores = BTreeMap::new();
        let mut changes = Vec::new();
        let mut spawned_entities = Vec::new();
        for effect in effects {
            let instance = match effect {
                crate::modules::WorldEffect::SpawnEntity { world, .. }
                | crate::modules::WorldEffect::DespawnEntity { world, .. }
                | crate::modules::WorldEffect::SetComponent { world, .. }
                | crate::modules::WorldEffect::RemoveComponent { world, .. } => *world,
                crate::modules::WorldEffect::RequestTransfer { .. } => continue,
            };
            if let Some(port) = authorization {
                validate_owner_access(registry, reservations, port, port.owner(), instance)?;
            }
            if let std::collections::btree_map::Entry::Vacant(entry) = stores.entry(instance) {
                let owner = self
                    .instances
                    .get(&instance)
                    .ok_or(RegistryError::UnknownInstance)?;
                entry.insert(owner.entities.clone());
            }
        }
        let untouched_entities: usize = self
            .instances
            .iter()
            .filter(|(id, _)| !stores.contains_key(id))
            .map(|(_, owner)| owner.entities.len())
            .sum();
        for effect in effects {
            match effect {
                crate::modules::WorldEffect::SpawnEntity {
                    world,
                    sector,
                    components,
                } => {
                    let total =
                        untouched_entities + stores.values().map(EntityStore::len).sum::<usize>();
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
                    spawned_entities.push((*world, entity));
                    changes.push(WorldEntityChange {
                        world: *world,
                        mutation: ComponentMutation::Spawn(entity),
                    });
                    let mut kinds = HashSet::new();
                    for component in components {
                        if !kinds.insert(component.kind) {
                            return Err(RegistryError::Component(ComponentError::InvalidPayload));
                        }
                        validate_component_mutation(
                            registry,
                            reservations,
                            authorization,
                            *world,
                            component.kind,
                        )?;
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
                        changes.push(WorldEntityChange {
                            world: *world,
                            mutation: ComponentMutation::Upsert(entity, component.kind, blob),
                        });
                    }
                }
                crate::modules::WorldEffect::DespawnEntity { world, entity } => {
                    let store = stores
                        .get_mut(world)
                        .ok_or(RegistryError::UnknownInstance)?;
                    let owns_protected_components = store
                        .iter_components()
                        .filter(|(id, _, _)| *id == *entity)
                        .all(|(_, kind, _)| {
                            validate_component_mutation(
                                registry,
                                reservations,
                                authorization,
                                *world,
                                kind,
                            )
                            .is_ok()
                        });
                    if reservations.is_reserved(*world, entity.get()) || !owns_protected_components
                    {
                        return Err(RegistryError::InvalidEntityState);
                    }
                    if !store.despawn(*entity).map_err(RegistryError::Component)? {
                        return Err(RegistryError::EntitySectorConflict);
                    }
                    changes.push(WorldEntityChange {
                        world: *world,
                        mutation: ComponentMutation::Despawn(*entity),
                    });
                }
                crate::modules::WorldEffect::SetComponent {
                    world,
                    entity,
                    component,
                } => {
                    validate_component_mutation(
                        registry,
                        reservations,
                        authorization,
                        *world,
                        component.kind,
                    )?;
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
                    changes.push(WorldEntityChange {
                        world: *world,
                        mutation: ComponentMutation::Upsert(*entity, component.kind, blob),
                    });
                }
                crate::modules::WorldEffect::RemoveComponent {
                    world,
                    entity,
                    kind,
                } => {
                    validate_component_mutation(
                        registry,
                        reservations,
                        authorization,
                        *world,
                        *kind,
                    )?;
                    let store = stores
                        .get_mut(world)
                        .ok_or(RegistryError::UnknownInstance)?;
                    if store
                        .remove_component(*entity, *kind)
                        .map_err(RegistryError::Component)?
                    {
                        changes.push(WorldEntityChange {
                            world: *world,
                            mutation: ComponentMutation::RemoveComponent(*entity, *kind),
                        });
                    }
                }
                crate::modules::WorldEffect::RequestTransfer { .. } => {}
            }
        }
        let membership_changed = stores.iter().any(|(instance, store)| {
            self.instances.get(instance).is_some_and(|owner| {
                owner.entities.membership_revision() != store.membership_revision()
            })
        });
        let next_membership_revision = if membership_changed {
            Some(
                self.next_entity_membership_revision()
                    .ok_or(RegistryError::MembershipRevisionExhausted)?,
            )
        } else {
            None
        };
        Ok(StagedEntityBatch {
            worlds: self,
            stores,
            changes,
            spawned_entities,
            next_membership_revision,
        })
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
        let membership_changed = staged.iter().any(|(instance, store)| {
            self.instances.get(instance).is_some_and(|owner| {
                owner.entities.membership_revision() != store.membership_revision()
            })
        });
        let next_membership_revision = if membership_changed {
            Some(
                self.next_entity_membership_revision()
                    .ok_or(RegistryError::MembershipRevisionExhausted)?,
            )
        } else {
            None
        };
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
        if let Some(revision) = next_membership_revision {
            self.entity_membership_revision = revision;
        }
        Ok(())
    }
}

fn validate_owner_access(
    registry: &ComponentRegistry,
    authority: &ComponentOwnerAuthority,
    port: &ComponentOwnerPort,
    owner: &minimium_world::entity::ComponentOwnerId,
    world: WorldInstanceId,
) -> Result<(), RegistryError> {
    if port.world() != world || !registry.contains_owner(owner) {
        return Err(RegistryError::InvalidEntityState);
    }
    authority
        .validate(port, owner, world)
        .map_err(|_| RegistryError::InvalidEntityState)
}

fn validate_component_mutation(
    registry: &ComponentRegistry,
    authority: &ComponentOwnerAuthority,
    authorization: Option<&ComponentOwnerPort>,
    world: WorldInstanceId,
    component: minimium_world::entity::ComponentTypeId,
) -> Result<(), RegistryError> {
    let Some(descriptor) = registry.descriptor(component) else {
        return Ok(());
    };
    if descriptor.mutation_policy == ComponentMutationPolicy::General {
        return Ok(());
    }
    let owner = registry
        .owner_for_component(component)
        .ok_or(RegistryError::InvalidEntityState)?;
    let port = authorization.ok_or(RegistryError::InvalidEntityState)?;
    if !registry.owns(port.owner(), component) {
        return Err(RegistryError::InvalidEntityState);
    }
    validate_owner_access(registry, authority, port, &owner, world)
}
