use super::changes::{ChangeLog, ComponentChange, ComponentMutation};
use super::registry::{ComponentError, ComponentRegistry};
use super::{
    ComponentTypeId, EntityId, SchemaVersion, MAX_COMPONENTS_PER_ENTITY, MAX_COMPONENT_BYTES,
    MAX_PERSISTENT_ENTITIES,
};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentBlob {
    pub schema: SchemaVersion,
    pub bytes: Vec<u8>,
}
impl ComponentBlob {
    pub fn new(schema: SchemaVersion, bytes: Vec<u8>) -> Option<Self> {
        (bytes.len() <= MAX_COMPONENT_BYTES).then_some(Self { schema, bytes })
    }
}

#[derive(Clone, Debug, Default)]
pub struct EntityStore {
    next_id: u64,
    membership_revision: u64,
    component_membership_revision: u64,
    entities: BTreeMap<EntityId, BTreeMap<ComponentTypeId, ComponentBlob>>,
    changes: ChangeLog,
}

impl EntityStore {
    pub fn spawn(&mut self) -> Result<EntityId, ComponentError> {
        if self.entities.len() >= MAX_PERSISTENT_ENTITIES {
            return Err(ComponentError::CapacityExceeded);
        }
        let next = self
            .next_id
            .checked_add(1)
            .ok_or(ComponentError::ExhaustedIds)?;
        let id = EntityId::new(next).ok_or(ComponentError::ExhaustedIds)?;
        self.next_revision()?;
        let membership_revision = self.next_membership_revision()?;
        self.next_id = next;
        self.entities.insert(id, BTreeMap::new());
        self.membership_revision = membership_revision;
        self.record(ComponentMutation::Spawn(id));
        Ok(id)
    }

    /// Rehydrate a pre-existing world-scoped ID while maintaining monotonic allocation.
    pub fn restore(&mut self, id: EntityId) -> Result<(), ComponentError> {
        if self.entities.len() >= MAX_PERSISTENT_ENTITIES {
            return Err(ComponentError::CapacityExceeded);
        }
        if self.entities.contains_key(&id) {
            return Err(ComponentError::DuplicateEntity);
        }
        let membership_revision = self.next_membership_revision()?;
        self.next_id = self.next_id.max(id.get());
        self.entities.insert(id, BTreeMap::new());
        self.membership_revision = membership_revision;
        Ok(())
    }

    pub fn contains(&self, id: EntityId) -> bool {
        self.entities.contains_key(&id)
    }
    pub fn len(&self) -> usize {
        self.entities.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }
    pub const fn next_id(&self) -> u64 {
        self.next_id
    }

    pub fn restore_counter(&mut self, next_id: u64) -> Result<(), ComponentError> {
        if next_id < self.next_id {
            return Err(ComponentError::InvalidPayload);
        }
        self.next_id = next_id;
        Ok(())
    }
    pub fn despawn(&mut self, id: EntityId) -> Result<bool, ComponentError> {
        if self.entities.contains_key(&id) {
            self.next_revision()?;
            let membership_revision = self.next_membership_revision()?;
            self.entities.remove(&id);
            self.membership_revision = membership_revision;
            self.record(ComponentMutation::Despawn(id));
            return Ok(true);
        }
        Ok(false)
    }
    pub fn iter(&self) -> impl Iterator<Item = EntityId> + '_ {
        self.entities.keys().copied()
    }

    /// Iterates components in stable entity/type ID order for persistence export.
    pub fn iter_components(
        &self,
    ) -> impl Iterator<Item = (EntityId, ComponentTypeId, &ComponentBlob)> + '_ {
        self.entities.iter().flat_map(|(entity, components)| {
            components
                .iter()
                .map(move |(kind, blob)| (*entity, *kind, blob))
        })
    }

    /// Iterates the latest live component updates in change-revision order.
    pub fn iter_changed_components(
        &self,
    ) -> impl Iterator<Item = (EntityId, ComponentTypeId, &ComponentBlob)> + '_ {
        self.changes.entries.iter().filter_map(|(revision, change)| {
            if let ComponentMutation::Upsert(entity, kind, blob) = &change.mutation {
                let superseded = self.changes.entries.range((revision.saturating_add(1))..).any(|(_, later)| {
                    matches!(&later.mutation,
                        ComponentMutation::Upsert(id, component, _) | ComponentMutation::RemoveComponent(id, component)
                            if id == entity && component == kind)
                        || matches!(&later.mutation, ComponentMutation::Despawn(id) if id == entity)
                });
                (!superseded && self.get(*entity, *kind) == Some(blob)).then_some((*entity, *kind, blob))
            } else {
                None
            }
        })
    }

    pub const fn revision(&self) -> u64 {
        self.changes.revision()
    }

    /// Advances only when entity membership changes, unlike `revision`, which
    /// also tracks component updates for replication.
    pub const fn membership_revision(&self) -> u64 {
        self.membership_revision
    }

    /// Advances when component membership changes, but not when an existing
    /// component's value is updated.
    pub const fn component_membership_revision(&self) -> u64 {
        self.component_membership_revision
    }

    /// The first revision still represented by the bounded replication journal.
    pub const fn oldest_change_revision(&self) -> u64 {
        self.changes.oldest_revision()
    }

    pub fn changes_since(&self, revision: u64) -> impl Iterator<Item = &ComponentChange> {
        self.changes.since(revision)
    }

    /// Restores a component and its explicit world-scoped entity ID.
    pub fn restore_component(
        &mut self,
        registry: &ComponentRegistry,
        entity: EntityId,
        kind: ComponentTypeId,
        schema: SchemaVersion,
        bytes: &[u8],
    ) -> Result<(), ComponentError> {
        let blob = registry.upgrade(kind, schema, bytes)?;
        let created_entity = !self.contains(entity);
        if created_entity && self.entities.len() >= MAX_PERSISTENT_ENTITIES {
            return Err(ComponentError::CapacityExceeded);
        }
        let had_component = self
            .entities
            .get(&entity)
            .is_some_and(|components| components.contains_key(&kind));
        if self.entities.get(&entity).is_some_and(|components| {
            !had_component && components.len() >= MAX_COMPONENTS_PER_ENTITY
        }) {
            return Err(ComponentError::CapacityExceeded);
        }
        let next_entity_revision = created_entity
            .then(|| self.next_membership_revision())
            .transpose()?;
        let next_component_revision = if had_component {
            None
        } else {
            Some(self.next_component_membership_revision()?)
        };
        if created_entity {
            self.next_id = self.next_id.max(entity.get());
            self.entities.insert(entity, BTreeMap::new());
            self.membership_revision =
                next_entity_revision.expect("new entity has a checked revision");
        }
        self.entities
            .get_mut(&entity)
            .ok_or(ComponentError::UnknownEntity)?
            .insert(kind, blob);
        if let Some(revision) = next_component_revision {
            self.component_membership_revision = revision;
        }
        Ok(())
    }

    pub fn insert(
        &mut self,
        registry: &ComponentRegistry,
        entity: EntityId,
        kind: ComponentTypeId,
        schema: SchemaVersion,
        bytes: &[u8],
    ) -> Result<(), ComponentError> {
        let blob = registry.upgrade(kind, schema, bytes)?;
        let components = self
            .entities
            .get(&entity)
            .ok_or(ComponentError::UnknownEntity)?;
        if !components.contains_key(&kind) && components.len() >= MAX_COMPONENTS_PER_ENTITY {
            return Err(ComponentError::CapacityExceeded);
        }
        self.next_revision()?;
        let next_component_revision = (!components.contains_key(&kind))
            .then(|| self.next_component_membership_revision())
            .transpose()?;
        self.entities
            .get_mut(&entity)
            .ok_or(ComponentError::UnknownEntity)?
            .insert(kind, blob.clone());
        self.record(ComponentMutation::Upsert(entity, kind, blob));
        if let Some(revision) = next_component_revision {
            self.component_membership_revision = revision;
        }
        Ok(())
    }

    pub fn remove_component(
        &mut self,
        entity: EntityId,
        kind: ComponentTypeId,
    ) -> Result<bool, ComponentError> {
        let components = self
            .entities
            .get(&entity)
            .ok_or(ComponentError::UnknownEntity)?;
        if !components.contains_key(&kind) {
            return Ok(false);
        }
        self.next_revision()?;
        let next_component_revision = self.next_component_membership_revision()?;
        self.entities
            .get_mut(&entity)
            .ok_or(ComponentError::UnknownEntity)?
            .remove(&kind);
        self.record(ComponentMutation::RemoveComponent(entity, kind));
        self.component_membership_revision = next_component_revision;
        Ok(true)
    }

    pub fn get(&self, entity: EntityId, kind: ComponentTypeId) -> Option<&ComponentBlob> {
        self.entities.get(&entity)?.get(&kind)
    }

    fn next_revision(&self) -> Result<u64, ComponentError> {
        self.changes.next_revision()
    }

    fn next_membership_revision(&self) -> Result<u64, ComponentError> {
        self.membership_revision
            .checked_add(1)
            .ok_or(ComponentError::RevisionExhausted)
    }

    fn next_component_membership_revision(&self) -> Result<u64, ComponentError> {
        self.component_membership_revision
            .checked_add(1)
            .ok_or(ComponentError::RevisionExhausted)
    }

    fn record(&mut self, mutation: ComponentMutation) {
        self.changes.record(mutation);
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        ComponentDescriptor, ComponentMutationPolicy, ReplicationPolicy, ReplicationVisibility,
    };
    use super::*;

    fn descriptor(id: u32, name: &'static str) -> ComponentDescriptor {
        ComponentDescriptor {
            id: ComponentTypeId::new(id).unwrap(),
            name,
            schema: SchemaVersion::new(2).unwrap(),
            validate_payload: |bytes| {
                (bytes.len() == 1)
                    .then_some(())
                    .ok_or(ComponentError::InvalidPayload)
            },
            replication: ReplicationPolicy::default(),
            visibility: ReplicationVisibility::Global,
            mutation_policy: ComponentMutationPolicy::General,
        }
    }

    #[test]
    fn registry_rejects_type_and_name_collisions() {
        let mut registry = ComponentRegistry::default();
        registry.register(descriptor(1, "item.stack")).unwrap();
        assert_eq!(
            registry.register(descriptor(1, "item.other")),
            Err(ComponentError::DuplicateType)
        );
        assert_eq!(
            registry.register(descriptor(2, "item.stack")),
            Err(ComponentError::DuplicateName)
        );
    }

    #[test]
    fn exact_schema_restore_and_tombstones_preserve_world_scoped_ids() {
        let mut registry = ComponentRegistry::default();
        registry.register(descriptor(1, "item.stack")).unwrap();
        let kind = ComponentTypeId::new(1).unwrap();
        let schema = SchemaVersion::new(2).unwrap();
        let mut store = EntityStore::default();
        let entity = EntityId::new(7).unwrap();
        store
            .restore_component(&registry, entity, kind, schema, &[9])
            .unwrap();
        assert_eq!(store.get(entity, kind).unwrap().bytes, vec![9]);
        assert_eq!(store.iter_changed_components().count(), 0);
        assert_eq!(store.spawn().unwrap().get(), 8);
        store.insert(&registry, entity, kind, schema, &[8]).unwrap();
        assert_eq!(store.iter_changed_components().count(), 1);
        assert!(store.despawn(entity).unwrap());
        assert_eq!(store.iter_changed_components().count(), 0);
    }
}
