//! Bounded client replica state. Full synchronizations commit atomically.

use minimium_protocol::{EntityReplicationOperation, WorldContext};
use minimium_world::entity::{ComponentTypeId, EntityId, SchemaVersion};
use std::collections::{BTreeMap, HashMap};

pub const MAX_REPLICA_ENTITIES: usize = minimium_world::entity::MAX_PERSISTENT_ENTITIES;
pub const MAX_REPLICA_COMPONENTS_PER_ENTITY: usize =
    minimium_world::entity::MAX_COMPONENTS_PER_ENTITY;
pub const MAX_REPLICA_BYTES: usize = minimium_world::entity::MAX_COMPONENT_PAYLOAD_BYTES;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplicaComponent {
    pub schema: SchemaVersion,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplicaEntity {
    pub components: BTreeMap<ComponentTypeId, ReplicaComponent>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplicatedEntities {
    pub context: Option<WorldContext>,
    pub revision: u64,
    pub entities: BTreeMap<EntityId, ReplicaEntity>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplicaError {
    InvalidSync,
    UnexpectedRevision,
    UnknownComponent,
    UnsupportedSchema,
    DuplicateEntity,
    UnknownEntity,
    ComponentLimit,
    EntityLimit,
    ByteLimit,
    SchemaConflict,
    AcknowledgementQueueFull,
}

impl std::fmt::Display for ReplicaError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "client entity replication rejected: {self:?}")
    }
}

impl std::error::Error for ReplicaError {}

pub(crate) struct ReplicaAssembly {
    pub context: WorldContext,
    pub sync_id: u64,
    pub target_revision: u64,
    pub state: ReplicatedEntities,
    pub payload_bytes: usize,
}

impl ReplicatedEntities {
    pub(crate) fn apply(
        &mut self,
        operation: &EntityReplicationOperation,
        schemas: &HashMap<ComponentTypeId, SchemaVersion>,
        payload_bytes: &mut usize,
    ) -> Result<(), ReplicaError> {
        match operation {
            EntityReplicationOperation::Spawn { entity } => {
                if self.entities.contains_key(entity) {
                    return Err(ReplicaError::DuplicateEntity);
                }
                if self.entities.len() >= MAX_REPLICA_ENTITIES {
                    return Err(ReplicaError::EntityLimit);
                }
                self.entities.insert(*entity, ReplicaEntity::default());
            }
            EntityReplicationOperation::ComponentUpsert {
                entity,
                kind,
                schema,
                bytes,
            } => {
                if bytes.len() > minimium_world::entity::MAX_COMPONENT_BYTES {
                    return Err(ReplicaError::ByteLimit);
                }
                let expected = schemas.get(kind).ok_or(ReplicaError::UnknownComponent)?;
                if expected != schema {
                    return Err(ReplicaError::UnsupportedSchema);
                }
                let replica = self
                    .entities
                    .get_mut(entity)
                    .ok_or(ReplicaError::UnknownEntity)?;
                if !replica.components.contains_key(kind)
                    && replica.components.len() >= MAX_REPLICA_COMPONENTS_PER_ENTITY
                {
                    return Err(ReplicaError::ComponentLimit);
                }
                let previous = replica
                    .components
                    .get(kind)
                    .map_or(0, |component| component.bytes.len());
                let next_size = (*payload_bytes)
                    .saturating_sub(previous)
                    .saturating_add(bytes.len());
                if next_size > MAX_REPLICA_BYTES {
                    return Err(ReplicaError::ByteLimit);
                }
                *payload_bytes = next_size;
                replica.components.insert(
                    *kind,
                    ReplicaComponent {
                        schema: *schema,
                        bytes: bytes.clone(),
                    },
                );
            }
            EntityReplicationOperation::ComponentRemove { entity, kind } => {
                let replica = self
                    .entities
                    .get_mut(entity)
                    .ok_or(ReplicaError::UnknownEntity)?;
                if let Some(component) = replica.components.remove(kind) {
                    *payload_bytes = (*payload_bytes).saturating_sub(component.bytes.len());
                }
            }
            EntityReplicationOperation::Despawn { entity } => {
                let replica = self
                    .entities
                    .remove(entity)
                    .ok_or(ReplicaError::UnknownEntity)?;
                for component in replica.components.values() {
                    *payload_bytes = (*payload_bytes).saturating_sub(component.bytes.len());
                }
            }
        }
        Ok(())
    }
}
