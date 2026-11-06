use crate::composition::{WorldDefinition, WorldPortal};
use minimium_core::ids::PlayerId;
use minimium_environment::{time::ClockId, EnvironmentProfile};
use minimium_simulation::World;
use minimium_world::world::WorldMap;
use minimium_world::{
    entity::{ComponentError, ComponentTypeId, EntityId},
    world::{
        GenerationRevision, GenerationSpec, GeneratorDigest, ObjectOriginId, SectorKey,
        WorldContentRevision, WorldDefinitionId, WorldInstanceId, WorldManifest, WorldSectorState,
        WorldSeed, WorldTopology,
    },
};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

mod accessors;
mod constructors;
mod diagnostics;
// Component persistence and replication staging are opt-in until a module uses them.
#[expect(dead_code, reason = "component state API is staged for opt-in modules")]
mod entity_state;
mod environment;
mod membership;
mod sector_state;
mod transfers;

use membership::Membership;
use transfers::PendingTransfer;
#[allow(unused_imports)] // Preserve the existing internal paths of these method result types.
pub(crate) use transfers::{CommittedTransfer, TransferOffer};

#[cfg(test)]
mod tests;

pub(crate) struct WorldInstance {
    pub(crate) clock_id: ClockId,
    pub(crate) environment_profile: EnvironmentProfile,
    pub(crate) manifest: WorldManifest,
    pub(crate) map: Arc<WorldMap>,
    pub(crate) world: World,
    /// Optional per-instance component state (legacy world simulation remains authoritative).
    pub(crate) entities: minimium_world::entity::EntityStore,
    pub(crate) portal: Option<WorldPortal>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    dead_code,
    reason = "component error cases are staged with the opt-in API"
)]
pub(crate) enum RegistryError {
    EmptyWorlds,
    DuplicateInstance,
    UnknownInstance,
    EpochSpaceExhausted,
    InvalidSectorState,
    StaleSectorRevision,
    SectorStateCapacity,
    Component(ComponentError),
    EntitySectorConflict,
    EntityOriginConflict,
    SectorRevisionExhausted,
}

/// Server-owned collection of isolated simulation owners and memberships.
pub(crate) struct WorldRegistry {
    instances: BTreeMap<WorldInstanceId, WorldInstance>,
    memberships: HashMap<PlayerId, Membership>,
    primary: WorldInstanceId,
    next_epoch: u64,
    transfers: HashMap<PlayerId, PendingTransfer>,
    next_transition: u128,
    sector_states: BTreeMap<(WorldInstanceId, minimium_world::world::SectorKey), WorldSectorState>,
    persisted_totals: sector_state::PersistedTotals,
    #[allow(
        dead_code,
        reason = "component sector ownership is staged for opt-in modules"
    )]
    entity_sectors: BTreeMap<(WorldInstanceId, EntityId), SectorKey>,
    #[allow(dead_code, reason = "component origins are staged for opt-in modules")]
    entity_origins: BTreeMap<(WorldInstanceId, EntityId, ComponentTypeId), ObjectOriginId>,
}
