use super::store::ComponentBlob;
use super::{ComponentTypeId, SchemaVersion, MAX_COMPONENT_BYTES};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ReplicationPolicy {
    #[default]
    ServerOnly,
    Replicated,
}

/// Declares how entities carrying a replicated component enter client interest.
#[derive(Clone, Copy)]
pub enum ReplicationVisibility {
    /// The component supplies a finite world position used for sector interest and ordering.
    Spatial {
        position: fn(&[u8]) -> Option<[f32; 3]>,
    },
    /// Explicitly opts the entity into all clients in the same world instance.
    Global,
}

#[derive(Clone, Copy)]
pub struct ComponentDescriptor {
    pub id: ComponentTypeId,
    pub name: &'static str,
    pub schema: SchemaVersion,
    pub replication: ReplicationPolicy,
    pub visibility: ReplicationVisibility,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComponentError {
    DuplicateType,
    DuplicateEntity,
    DuplicateName,
    UnknownType,
    UnsupportedSchema,
    InvalidPayload,
    UnknownEntity,
    ExhaustedIds,
    CapacityExceeded,
    RevisionExhausted,
}

#[derive(Default)]
pub struct ComponentRegistry {
    types: BTreeMap<ComponentTypeId, ComponentDescriptor>,
}

impl ComponentRegistry {
    pub fn register(&mut self, descriptor: ComponentDescriptor) -> Result<(), ComponentError> {
        if self.types.contains_key(&descriptor.id) {
            return Err(ComponentError::DuplicateType);
        }
        if self
            .types
            .values()
            .any(|existing| existing.name == descriptor.name)
        {
            return Err(ComponentError::DuplicateName);
        }
        if descriptor.name.is_empty()
            || descriptor.name.len() > 96
            || !descriptor
                .name
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'.' || c == b'_')
        {
            return Err(ComponentError::InvalidPayload);
        }
        self.types.insert(descriptor.id, descriptor);
        Ok(())
    }

    pub fn descriptor(&self, id: ComponentTypeId) -> Option<&ComponentDescriptor> {
        self.types.get(&id)
    }

    pub fn upgrade(
        &self,
        id: ComponentTypeId,
        schema: SchemaVersion,
        bytes: &[u8],
    ) -> Result<ComponentBlob, ComponentError> {
        let descriptor = self.descriptor(id).ok_or(ComponentError::UnknownType)?;
        if bytes.len() > MAX_COMPONENT_BYTES {
            return Err(ComponentError::InvalidPayload);
        }
        if schema != descriptor.schema {
            return Err(ComponentError::UnsupportedSchema);
        }
        ComponentBlob::new(descriptor.schema, bytes.to_vec()).ok_or(ComponentError::InvalidPayload)
    }
}
