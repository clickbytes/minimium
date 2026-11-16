use minimium_procgen::{SectorContentVersion, SectorGeometry, TerrainInputs};
use minimium_protocol::WorldContext;
use minimium_simulation::CollisionIndex;
use minimium_world::world::{SectorKey, TransitionId, WorldInstanceId, WorldManifest, WorldMap};
use std::sync::Arc;

/// Distinct bootstrap, active membership and pending transfer interests.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorldReadinessOwner {
    Bootstrap(WorldInstanceId),
    Active(WorldContext),
    Pending(TransitionId, WorldContext),
}

impl WorldReadinessOwner {
    pub const fn instance(self) -> WorldInstanceId {
        match self {
            Self::Bootstrap(instance) => instance,
            Self::Active(context) | Self::Pending(_, context) => context.instance(),
        }
    }

    pub const fn context(self) -> Option<WorldContext> {
        match self {
            Self::Bootstrap(_) => None,
            Self::Active(context) | Self::Pending(_, context) => Some(context),
        }
    }

    pub(super) fn matches_manifest(self, manifest: WorldManifest) -> bool {
        self.context()
            .is_none_or(|context| context.content_revision() == manifest.content_revision())
    }
}

/// CPU-only result certified by WorldReadiness from its installed sector tickets.
/// No constructor or mutable fields allow a host to relabel collision readiness.
#[derive(Clone)]
pub struct CollisionSnapshot {
    pub(super) owner: WorldReadinessOwner,
    pub(super) manifest: WorldManifest,
    pub(super) map_fingerprint: u64,
    pub(super) terrain_revision: SectorContentVersion,
    pub(super) collision_revision: u64,
    pub(super) position: [f32; 3],
    pub(super) sectors: Arc<[(SectorKey, Arc<SectorGeometry>)]>,
    pub(super) collision: CollisionIndex,
    pub(super) terrain_inputs: TerrainInputs,
}

impl CollisionSnapshot {
    pub fn owner(&self) -> WorldReadinessOwner {
        self.owner
    }
    pub fn instance(&self) -> WorldInstanceId {
        self.owner.instance()
    }
    pub fn context(&self) -> Option<WorldContext> {
        self.owner.context()
    }
    pub fn manifest(&self) -> WorldManifest {
        self.manifest
    }
    pub fn map_fingerprint(&self) -> u64 {
        self.map_fingerprint
    }
    pub fn terrain_revision(&self) -> SectorContentVersion {
        self.terrain_revision
    }
    pub fn collision_revision(&self) -> u64 {
        self.collision_revision
    }
    pub fn position(&self) -> [f32; 3] {
        self.position
    }
    pub fn sectors(&self) -> &[(SectorKey, Arc<SectorGeometry>)] {
        &self.sectors
    }
    pub fn collision(&self) -> &CollisionIndex {
        &self.collision
    }

    pub fn matches(
        &self,
        owner: WorldReadinessOwner,
        manifest: WorldManifest,
        map: &WorldMap,
        terrain: &TerrainInputs,
    ) -> bool {
        self.owner == owner
            && self.manifest == manifest
            && self.map_fingerprint == map.fingerprint()
            && self.matches_terrain(terrain)
            && terrain
                .source()
                .matches_map(owner.instance(), manifest, map)
            && terrain.overlays().world() == Some(owner.instance())
    }

    /// Checks the immutable source and independent overlay revisions consumed
    /// by a derived presentation or collision consumer.
    pub fn matches_terrain(&self, terrain: &TerrainInputs) -> bool {
        self.terrain_revision == terrain.revision()
            && same_source(&self.terrain_inputs, terrain)
            && terrain.overlays().world() == Some(self.instance())
    }

    /// Covers the existing fixed-step collision neighborhood at this position.
    /// This does not certify an arbitrary sweep, capsule extent or water region.
    /// A different required neighborhood must be requested from the provider.
    pub fn covers_position(&self, position: [f32; 3]) -> bool {
        valid_position(self.manifest, position)
            && minimium_procgen::sector_neighborhood_for(self.manifest, position)
                .iter()
                .all(|key| self.sectors.iter().any(|(present, _)| present == key))
    }
}

pub(super) fn same_source(left: &TerrainInputs, right: &TerrainInputs) -> bool {
    let left = left.source();
    let right = right.source();
    left.world() == right.world()
        && left.manifest() == right.manifest()
        && left.map_fingerprint() == right.map_fingerprint()
        && left.material_profile() == right.material_profile()
        && left.relief_policy() == right.relief_policy()
        && left.geography().map(|geo| geo.package_digest())
            == right.geography().map(|geo| geo.package_digest())
}

pub(super) fn valid_position(manifest: WorldManifest, position: [f32; 3]) -> bool {
    if position.iter().any(|value| !value.is_finite()) {
        return false;
    }
    match manifest.topology() {
        minimium_world::world::WorldTopology::Planet => position.iter().any(|value| *value != 0.0),
        minimium_world::world::WorldTopology::Dungeon => position.iter().all(|value| {
            let sector =
                (f64::from(*value) / f64::from(minimium_procgen::DUNGEON_SECTOR_EDGE)).floor();
            sector >= f64::from(i32::MIN) + 1.0 && sector <= f64::from(i32::MAX) - 1.0
        }),
    }
}
