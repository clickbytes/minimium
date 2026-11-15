//! World-scoped entity/component contracts and bounded change tracking.

pub const MAX_COMPONENT_BYTES: usize = 2 * 1024;
pub const MAX_COMPONENTS_PER_ENTITY: usize = 16;
pub const MAX_PERSISTENT_ENTITIES: usize = 16_384;
pub const MAX_COMPONENT_PAYLOAD_BYTES: usize = 4 * 1024 * 1024;

mod changes;
mod ids;
mod registry;
mod store;

pub use changes::{ComponentChange, ComponentMutation};
pub use ids::{ComponentTypeId, EntityId, SchemaVersion};
pub use registry::{
    ComponentDescriptor, ComponentError, ComponentRegistry, ReplicationPolicy,
    ReplicationVisibility,
};
pub use store::{ComponentBlob, EntityStore};
