use super::store::ComponentBlob;
use super::{ComponentTypeId, SchemaVersion, MAX_COMPONENT_BYTES};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ReplicationPolicy {
    #[default]
    ServerOnly,
    Replicated,
}

/// Declares whether ordinary host/module entity effects may mutate this component.
/// Ownership is bound by registry registration, never supplied by an effect.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ComponentMutationPolicy {
    #[default]
    General,
    OwnerOnly,
}

/// Opaque identity minted by one component registry for a validated owner registration.
#[derive(Clone, Debug)]
pub struct ComponentOwnerId(Arc<OwnerIdentity>);

#[derive(Debug)]
struct OwnerIdentity {
    id: u64,
    registry: Arc<RegistryIdentity>,
}

#[derive(Debug)]
struct RegistryIdentity {
    _marker: u8,
}

impl ComponentOwnerId {
    pub fn get(&self) -> u64 {
        self.0.id
    }
}

impl PartialEq for ComponentOwnerId {
    fn eq(&self, other: &Self) -> bool {
        self.0.id == other.0.id && Arc::ptr_eq(&self.0.registry, &other.0.registry)
    }
}

impl Eq for ComponentOwnerId {}

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
    /// Validates this schema's encoded payload after size and schema checks.
    pub validate_payload: fn(&[u8]) -> Result<(), ComponentError>,
    pub replication: ReplicationPolicy,
    pub visibility: ReplicationVisibility,
    pub mutation_policy: ComponentMutationPolicy,
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
    MissingOwner,
    InconsistentOwner,
    InvalidOwner,
    OwnerIdExhausted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OwnerOrigin {
    Builtin,
    Module,
}

#[derive(Clone, Copy, Debug)]
struct RegisteredOwner {
    id: u64,
    origin: OwnerOrigin,
}

pub struct ComponentRegistry {
    types: BTreeMap<ComponentTypeId, ComponentDescriptor>,
    owners: BTreeMap<ComponentTypeId, u64>,
    owner_names: BTreeMap<&'static str, RegisteredOwner>,
    next_owner_id: u64,
    identity: Arc<RegistryIdentity>,
}

impl Default for ComponentRegistry {
    fn default() -> Self {
        Self {
            types: BTreeMap::new(),
            owners: BTreeMap::new(),
            owner_names: BTreeMap::new(),
            next_owner_id: 0,
            identity: Arc::new(RegistryIdentity { _marker: 0 }),
        }
    }
}

impl ComponentRegistry {
    pub fn register(&mut self, descriptor: ComponentDescriptor) -> Result<(), ComponentError> {
        if descriptor.mutation_policy == ComponentMutationPolicy::OwnerOnly {
            return Err(ComponentError::MissingOwner);
        }
        self.register_descriptor(descriptor, None)
    }

    /// Binds a built-in owner's descriptor to one registry-minted identity.
    /// Repeated registrations under the same built-in owner share that identity.
    pub(crate) fn register_builtin(
        &mut self,
        owner_name: &'static str,
        descriptor: ComponentDescriptor,
    ) -> Result<ComponentOwnerId, ComponentError> {
        self.register_for_owner(owner_name, OwnerOrigin::Builtin, descriptor)
    }

    /// Binds a module descriptor to the module identity validated by ModuleRegistry.
    /// A module cannot claim a built-in owner's identity.
    pub fn register_module_component(
        &mut self,
        module: &'static str,
        descriptor: ComponentDescriptor,
    ) -> Result<Option<ComponentOwnerId>, ComponentError> {
        if descriptor.mutation_policy == ComponentMutationPolicy::General {
            self.register_descriptor(descriptor, None)?;
            return Ok(None);
        }
        self.register_for_owner(module, OwnerOrigin::Module, descriptor)
            .map(Some)
    }

    fn register_for_owner(
        &mut self,
        owner_name: &'static str,
        origin: OwnerOrigin,
        descriptor: ComponentDescriptor,
    ) -> Result<ComponentOwnerId, ComponentError> {
        if descriptor.mutation_policy != ComponentMutationPolicy::OwnerOnly {
            return Err(ComponentError::InconsistentOwner);
        }
        if !valid_owner_name(owner_name) {
            return Err(ComponentError::InvalidOwner);
        }
        let existing = match self.owner_names.get(owner_name) {
            Some(existing) if existing.origin == origin => Some(existing.id),
            Some(_) => return Err(ComponentError::InconsistentOwner),
            None => None,
        };
        let owner = match existing {
            Some(owner) => owner,
            None => {
                let id = self
                    .next_owner_id
                    .checked_add(1)
                    .ok_or(ComponentError::OwnerIdExhausted)?;
                id
            }
        };
        self.validate_descriptor(descriptor, Some(owner))?;
        if existing.is_none() {
            self.next_owner_id = owner;
            self.owner_names
                .insert(owner_name, RegisteredOwner { id: owner, origin });
        }
        self.register_descriptor(descriptor, Some(owner))?;
        Ok(self.owner_id(owner))
    }

    fn register_descriptor(
        &mut self,
        descriptor: ComponentDescriptor,
        owner: Option<u64>,
    ) -> Result<(), ComponentError> {
        self.validate_descriptor(descriptor, owner)?;
        self.types.insert(descriptor.id, descriptor);
        if let Some(owner) = owner {
            self.owners.insert(descriptor.id, owner);
        }
        Ok(())
    }

    fn validate_descriptor(
        &self,
        descriptor: ComponentDescriptor,
        owner: Option<u64>,
    ) -> Result<(), ComponentError> {
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
        if (descriptor.mutation_policy == ComponentMutationPolicy::OwnerOnly) != owner.is_some() {
            return Err(if owner.is_some() {
                ComponentError::InconsistentOwner
            } else {
                ComponentError::MissingOwner
            });
        }
        Ok(())
    }

    pub fn descriptor(&self, id: ComponentTypeId) -> Option<&ComponentDescriptor> {
        self.types.get(&id)
    }

    pub fn owner_for_component(&self, id: ComponentTypeId) -> Option<ComponentOwnerId> {
        self.owners.get(&id).map(|owner| self.owner_id(*owner))
    }

    pub fn owns(&self, owner: &ComponentOwnerId, component: ComponentTypeId) -> bool {
        Arc::ptr_eq(&self.identity, &owner.0.registry)
            && self.owners.get(&component) == Some(&owner.0.id)
    }

    pub fn contains_owner(&self, owner: &ComponentOwnerId) -> bool {
        Arc::ptr_eq(&self.identity, &owner.0.registry)
            && self
                .owner_names
                .values()
                .any(|registered| registered.id == owner.0.id)
    }

    pub fn is_module_owner(&self, owner: &ComponentOwnerId) -> bool {
        Arc::ptr_eq(&self.identity, &owner.0.registry)
            && self.owner_names.values().any(|registered| {
                registered.id == owner.0.id && registered.origin == OwnerOrigin::Module
            })
    }

    fn owner_id(&self, id: u64) -> ComponentOwnerId {
        ComponentOwnerId(Arc::new(OwnerIdentity {
            id,
            registry: self.identity.clone(),
        }))
    }

    pub fn upgrade(
        &self,
        id: ComponentTypeId,
        schema: SchemaVersion,
        bytes: &[u8],
    ) -> Result<ComponentBlob, ComponentError> {
        let descriptor = self.descriptor(id).ok_or(ComponentError::UnknownType)?;
        match descriptor.mutation_policy {
            ComponentMutationPolicy::General if self.owners.contains_key(&id) => {
                return Err(ComponentError::InconsistentOwner);
            }
            ComponentMutationPolicy::OwnerOnly if !self.owners.contains_key(&id) => {
                return Err(ComponentError::MissingOwner);
            }
            _ => {}
        }
        if bytes.len() > MAX_COMPONENT_BYTES {
            return Err(ComponentError::InvalidPayload);
        }
        if schema != descriptor.schema {
            return Err(ComponentError::UnsupportedSchema);
        }
        (descriptor.validate_payload)(bytes)?;
        ComponentBlob::new(descriptor.schema, bytes.to_vec()).ok_or(ComponentError::InvalidPayload)
    }
}

fn valid_owner_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 96
        && name.split(' ').all(|part| {
            !part.is_empty()
                && part.bytes().all(|c| {
                    c.is_ascii_lowercase()
                        || c.is_ascii_digit()
                        || c == b'.'
                        || c == b'_'
                        || c == b'-'
                })
        })
}
