use crate::composition::WorldDefinition;
use minimium_core::ids::PlayerId;
use minimium_environment::{time::ClockId, EnvironmentProfile};
use minimium_simulation::World;
use minimium_world::world::WorldMap;
use minimium_world::{
    entity::{ComponentError, ComponentTypeId, EntityId},
    world::{
        GenerationRevision, GenerationSpec, GeneratorDigest, SectorKey, WorldContentRevision,
        WorldDefinitionId, WorldInstanceId, WorldManifest, WorldSectorState, WorldSeed,
        WorldTopology,
    },
};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

mod accessors;
mod constructors;
mod diagnostics;
mod entity_state;
mod environment;
mod membership;
mod placement;
mod sector_state;
mod transfers;

use membership::Membership;
pub(crate) use placement::{terrain_capsule_clear, TerrainReads};
use transfers::PendingTransfer;
pub(crate) use transfers::{CancelledTransfer, CommittedTransfer, TransferRequestRejection};

#[cfg(test)]
mod tests;

pub(crate) struct WorldInstance {
    pub(crate) collision_tickets: std::collections::HashSet<minimium_sectors::SectorTicket>,
    pub(crate) terrain: Arc<minimium_terrain::TerrainSnapshot>,
    pub(crate) volumes: Option<Arc<minimium_terrain::TerrainStore>>,
    pub(crate) remnants: Arc<minimium_world::remnants::RemnantSnapshot>,
    pub(crate) clock_id: ClockId,
    pub(crate) environment_profile: EnvironmentProfile,
    pub(crate) manifest: WorldManifest,
    pub(crate) terrain_material_profile: minimium_procgen::ResolvedTerrainMaterialProfile,
    pub(crate) resolved_relief_policy: Option<minimium_procgen::ResolvedPlanetReliefPolicy>,
    pub(crate) map: Arc<WorldMap>,
    pub(crate) world: World,
    pub(crate) collision_source_revision: u64,
    pub(crate) collision_installed_revision: u64,
    /// Persistent game entities owned by this world instance.
    pub(crate) entities: minimium_world::entity::EntityStore,
    pub(crate) content_initialized: bool,
    pub(crate) initial_entities: Vec<crate::composition::InitialEntity>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RegistryError {
    EmptyWorlds,
    DuplicateInstance,
    WorldLimitExceeded,
    UnknownInstance,
    EpochSpaceExhausted,
    InvalidSectorState,
    InvalidManifest,
    InvalidTerrainMaterialProfile,
    InvalidReliefPolicy,
    InvalidEntityState,
    ProtectedComponent,
    EntityReservationConflict,
    StaleSectorRevision,
    SectorStateCapacity,
    Component(ComponentError),
    EntitySectorConflict,
    SnapshotBudgetExceeded,
    MembershipRevisionExhausted,
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
    pub(crate) player_membership_revision: u128,
    pub(crate) read_identity: std::sync::Arc<()>,
    pub(crate) world_membership_revision: u128,
    pub(crate) entity_membership_revision: u128,
}

impl WorldRegistry {
    pub(crate) fn next_entity_membership_revision(&self) -> Option<u128> {
        self.entity_membership_revision.checked_add(1)
    }
    pub(crate) fn next_player_membership_revision(&self) -> Option<u128> {
        self.player_membership_revision.checked_add(1)
    }
}
