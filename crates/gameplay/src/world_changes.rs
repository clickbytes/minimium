//! Bounded neutral plans for coherent, persistence-backed world changes.
use minimium_core::identity::PlayerProfileId;
use minimium_fluids::FluidSnapshot;
use minimium_terrain::ChunkImage;
use minimium_world::{
    impact::BodyState,
    items::Inventory,
    terrain::{TerrainChunkKey, TerrainRevision},
    terrain_edits::CraterStamp,
    world::WorldInstanceId,
};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

pub const MAX_WORLD_CHANGE_CHUNKS: usize = 64;
pub const MAX_WORLD_CHANGE_BODIES: usize = 4096;

/// An immutable gameplay-owned envelope. Clones retain the same opaque identity,
/// allowing an adapter to acknowledge exactly the plan it accepted.
#[derive(Clone)]
pub struct WorldChangePlan {
    identity: Arc<()>,
    id: u64,
    world: WorldInstanceId,
    world_seed: u128,
    expected_chunk_revisions: Vec<(TerrainChunkKey, TerrainRevision)>,
    resulting_chunks: Vec<Arc<ChunkImage>>,
    inventory: Option<(PlayerProfileId, Inventory)>,
    fluids: Option<FluidSnapshot>,
    impact_bodies: Vec<BodyState>,
    craters: Vec<CraterStamp>,
    water_catalog_digest: Option<[u8; 32]>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldChangePlanError {
    InvalidIdentity,
    LimitExceeded,
    EmptyChange,
    MismatchedWorld,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldChangeFailure {
    IdentityConflict,
    RevisionConflict,
    LimitExceeded,
    InvalidPlan,
    Busy,
    Unavailable,
    Failure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldChangeCommitment {
    Durable,
    Volatile,
}

pub struct WorldChangeAck {
    pub plan: WorldChangePlan,
    pub commitment: WorldChangeCommitment,
}

impl WorldChangePlan {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: u64,
        world: WorldInstanceId,
        world_seed: u128,
        expected_chunk_revisions: Vec<(TerrainChunkKey, TerrainRevision)>,
        resulting_chunks: Vec<Arc<ChunkImage>>,
        inventory: Option<(PlayerProfileId, Inventory)>,
        fluids: Option<FluidSnapshot>,
        impact_bodies: Vec<BodyState>,
        craters: Vec<CraterStamp>,
        water_catalog_digest: Option<[u8; 32]>,
    ) -> Result<Self, WorldChangePlanError> {
        if id == 0 {
            return Err(WorldChangePlanError::InvalidIdentity);
        }
        if expected_chunk_revisions.len() > MAX_WORLD_CHANGE_CHUNKS
            || resulting_chunks.len() > MAX_WORLD_CHANGE_CHUNKS
            || impact_bodies.len() > MAX_WORLD_CHANGE_BODIES
            || craters.len() > minimium_world::terrain_edits::MAX_TERRAIN_EDITS
        {
            return Err(WorldChangePlanError::LimitExceeded);
        }
        if resulting_chunks.is_empty()
            && inventory.is_none()
            && fluids.is_none()
            && impact_bodies.is_empty()
            && water_catalog_digest.is_none()
        {
            return Err(WorldChangePlanError::EmptyChange);
        }
        if fluids
            .as_ref()
            .is_some_and(|snapshot| snapshot.world() != world)
            || resulting_chunks
                .iter()
                .any(|chunk| chunk.key().world != world)
            || expected_chunk_revisions
                .iter()
                .any(|(key, _)| key.world != world)
            || impact_bodies.iter().any(|body| body.world != world)
        {
            return Err(WorldChangePlanError::MismatchedWorld);
        }
        Ok(Self {
            identity: Arc::new(()),
            id,
            world,
            world_seed,
            expected_chunk_revisions,
            resulting_chunks,
            inventory,
            fluids,
            impact_bodies,
            craters,
            water_catalog_digest,
        })
    }

    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn world(&self) -> WorldInstanceId {
        self.world
    }
    pub fn world_seed(&self) -> u128 {
        self.world_seed
    }
    pub fn expected_chunk_revisions(&self) -> &[(TerrainChunkKey, TerrainRevision)] {
        &self.expected_chunk_revisions
    }
    pub fn resulting_chunks(&self) -> &[Arc<ChunkImage>] {
        &self.resulting_chunks
    }
    pub fn inventory(&self) -> Option<(PlayerProfileId, Inventory)> {
        self.inventory
    }
    pub fn fluids(&self) -> Option<&FluidSnapshot> {
        self.fluids.as_ref()
    }
    pub fn impact_bodies(&self) -> &[BodyState] {
        &self.impact_bodies
    }
    pub fn craters(&self) -> &[CraterStamp] {
        &self.craters
    }
    pub fn water_catalog_digest(&self) -> Option<[u8; 32]> {
        self.water_catalog_digest
    }

    /// True only for clones of this exact immutable envelope.
    pub fn same_plan(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.identity, &other.identity)
    }
}

#[derive(Clone, Default)]
pub struct RestoredWorldChanges {
    pub world_seed: u128,
    pub last_mutation: u64,
    pub chunks: BTreeMap<[i32; 3], Arc<ChunkImage>>,
    pub fluids: Option<FluidSnapshot>,
    pub impact_bodies: BTreeMap<u64, BodyState>,
    pub craters: BTreeMap<u64, CraterStamp>,
    pub water_catalog_digest: Option<[u8; 32]>,
}

/// Neutral materialized state; replay offsets and record metadata stay in storage.
#[derive(Clone, Default)]
pub struct WorldChangeState {
    pub worlds: BTreeMap<WorldInstanceId, RestoredWorldChanges>,
    pub inventories: HashMap<PlayerProfileId, Inventory>,
    pub last_mutation_id: u64,
    pub mutation_count: u64,
}
