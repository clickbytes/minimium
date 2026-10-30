//! Minimal world-scoped entity/component store without a full ECS dependency.
//! Entity IDs are meaningful only within their owning WorldInstanceId.
//! Durable component blobs use an opt-in envelope; unrecognized old blobs stay
//! owned by their existing serializers.
use crate::world::{ObjectOriginId, WorldObjectState, MAX_OBJECT_STATE_BYTES};
use std::collections::BTreeMap;

pub const MAX_COMPONENT_BYTES: usize = 16 * 1024;
const PERSISTENCE_MAGIC: [u8; 4] = *b"MEC1";
const HEADER_BYTES: usize = 4 + 8 + 4 + 4;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EntityId(u64);
impl EntityId {
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }
    pub const fn get(self) -> u64 { self.0 }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ComponentTypeId(u32);
impl ComponentTypeId {
    pub const fn new(value: u32) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }
    pub const fn get(self) -> u32 { self.0 }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SchemaVersion(u32);
impl SchemaVersion {
    pub const fn new(value: u32) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }
    pub const fn get(self) -> u32 { self.0 }
}

pub type SchemaMigration = fn(SchemaVersion, &[u8]) -> Option<Vec<u8>>;

#[derive(Clone, Copy)]
pub struct ComponentDescriptor {
    pub id: ComponentTypeId,
    pub name: &'static str,
    pub schema: SchemaVersion,
    /// Must deterministically upgrade all supported previous versions.
    pub migrate: Option<SchemaMigration>,
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
}

#[derive(Default)]
pub struct ComponentRegistry {
    types: BTreeMap<ComponentTypeId, ComponentDescriptor>,
}

impl ComponentRegistry {
    pub fn register(&mut self, descriptor: ComponentDescriptor) -> Result<(), ComponentError> {
        if self.types.contains_key(&descriptor.id) { return Err(ComponentError::DuplicateType); }
        if self.types.values().any(|existing| existing.name == descriptor.name) {
            return Err(ComponentError::DuplicateName);
        }
        if descriptor.name.is_empty() || descriptor.name.len() > 96 ||
            !descriptor.name.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'.' || c == b'_')
        { return Err(ComponentError::InvalidPayload); }
        self.types.insert(descriptor.id, descriptor);
        Ok(())
    }

    pub fn descriptor(&self, id: ComponentTypeId) -> Option<&ComponentDescriptor> {
        self.types.get(&id)
    }

    pub fn upgrade(
        &self, id: ComponentTypeId, schema: SchemaVersion, bytes: &[u8],
    ) -> Result<ComponentBlob, ComponentError> {
        let descriptor = self.descriptor(id).ok_or(ComponentError::UnknownType)?;
        if bytes.len() > MAX_COMPONENT_BYTES { return Err(ComponentError::InvalidPayload); }
        let payload = if schema == descriptor.schema {
            bytes.to_vec()
        } else if schema.get() < descriptor.schema.get() {
            let migrate = descriptor.migrate.ok_or(ComponentError::UnsupportedSchema)?;
            migrate(schema, bytes).ok_or(ComponentError::UnsupportedSchema)?
        } else {
            return Err(ComponentError::UnsupportedSchema);
        };
        ComponentBlob::new(descriptor.schema, payload).ok_or(ComponentError::InvalidPayload)
    }
}

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

#[derive(Debug, Default)]
pub struct EntityStore {
    next_id: u64,
    entities: BTreeMap<EntityId, BTreeMap<ComponentTypeId, ComponentBlob>>,
}

impl EntityStore {
    pub fn spawn(&mut self) -> Result<EntityId, ComponentError> {
        let next = self.next_id.checked_add(1).ok_or(ComponentError::ExhaustedIds)?;
        let id = EntityId::new(next).ok_or(ComponentError::ExhaustedIds)?;
        self.next_id = next;
        self.entities.insert(id, BTreeMap::new());
        Ok(id)
    }

    /// Rehydrate a pre-existing world-scoped ID while maintaining monotonic allocation.
    pub fn restore(&mut self, id: EntityId) -> Result<(), ComponentError> {
        if self.entities.contains_key(&id) { return Err(ComponentError::DuplicateEntity); }
        self.next_id = self.next_id.max(id.get());
        self.entities.insert(id, BTreeMap::new());
        Ok(())
    }

    pub fn contains(&self, id: EntityId) -> bool { self.entities.contains_key(&id) }
    pub fn len(&self) -> usize { self.entities.len() }
    pub fn is_empty(&self) -> bool { self.entities.is_empty() }
    pub fn despawn(&mut self, id: EntityId) -> bool { self.entities.remove(&id).is_some() }
    pub fn iter(&self) -> impl Iterator<Item = EntityId> + '_ { self.entities.keys().copied() }

    pub fn insert(
        &mut self, registry: &ComponentRegistry, entity: EntityId, kind: ComponentTypeId,
        schema: SchemaVersion, bytes: &[u8],
    ) -> Result<(), ComponentError> {
        let blob = registry.upgrade(kind, schema, bytes)?;
        self.entities.get_mut(&entity).ok_or(ComponentError::UnknownEntity)?
            .insert(kind, blob);
        Ok(())
    }

    pub fn get(&self, entity: EntityId, kind: ComponentTypeId) -> Option<&ComponentBlob> {
        self.entities.get(&entity)?.get(&kind)
    }

    /// An opt-in versioned envelope in the existing opaque object data field.
    /// This does not alter the on-disk layout nor reinterpret legacy objects.
    pub fn to_world_object(
        &self, origin: ObjectOriginId, entity: EntityId, kind: ComponentTypeId,
    ) -> Option<WorldObjectState> {
        let blob = self.get(entity, kind)?;
        if blob.bytes.len() + HEADER_BYTES > MAX_OBJECT_STATE_BYTES { return None; }
        let mut bytes = Vec::with_capacity(blob.bytes.len() + HEADER_BYTES);
        bytes.extend_from_slice(&PERSISTENCE_MAGIC);
        bytes.extend_from_slice(&entity.get().to_le_bytes());
        bytes.extend_from_slice(&kind.get().to_le_bytes());
        bytes.extend_from_slice(&blob.schema.get().to_le_bytes());
        bytes.extend_from_slice(&blob.bytes);
        WorldObjectState::new(origin, bytes)
    }

    /// Returns None for a legacy blob so its previous owner may decode it.
    /// A recognized but malformed envelope is rejected instead of guessing.
    pub fn decode_world_object(
        registry: &ComponentRegistry, object: &WorldObjectState,
    ) -> Result<Option<(EntityId, ComponentTypeId, ComponentBlob)>, ComponentError> {
        let data = object.data();
        if !data.starts_with(&PERSISTENCE_MAGIC) { return Ok(None); }
        if data.len() < HEADER_BYTES { return Err(ComponentError::InvalidPayload); }
        let entity = EntityId::new(u64::from_le_bytes(data[4..12].try_into().unwrap()))
            .ok_or(ComponentError::InvalidPayload)?;
        let kind = ComponentTypeId::new(u32::from_le_bytes(data[12..16].try_into().unwrap()))
            .ok_or(ComponentError::InvalidPayload)?;
        let schema = SchemaVersion::new(u32::from_le_bytes(data[16..20].try_into().unwrap()))
            .ok_or(ComponentError::InvalidPayload)?;
        let upgraded = registry.upgrade(kind, schema, &data[HEADER_BYTES..])?;
        Ok(Some((entity, kind, upgraded)))
    }
}

