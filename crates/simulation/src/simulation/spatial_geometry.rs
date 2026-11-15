use super::collision_index::CollisionIndex;
use minimium_world::world::SectorKey;
use std::{collections::BTreeMap, sync::Arc};

/// Closed interval of one exact planet sector domain along a shot ray.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CombatRaySectorInterval {
    pub key: SectorKey,
    pub start_m: f32,
    pub end_m: f32,
}

/// Coherent immutable geometry inputs for movement and full-range combat.
#[derive(Clone)]
pub struct SpatialGeometryPublication {
    pub(crate) revision: u64,
    pub(crate) movement: Arc<CollisionIndex>,
    pub(crate) combat_baseline: Arc<CollisionIndex>,
    pub(crate) combat_replacements: BTreeMap<SectorKey, Arc<CollisionIndex>>,
}

impl SpatialGeometryPublication {
    pub(crate) fn with_local_collider(&mut self, collider: minimium_world::world::Aabb) {
        Arc::make_mut(&mut self.movement).add(collider);
        Arc::make_mut(&mut self.combat_baseline).add(collider);
        self.revision = self.revision.saturating_add(1);
    }
}

impl Default for SpatialGeometryPublication {
    fn default() -> Self {
        Self::new(0, Arc::default(), Arc::default(), BTreeMap::new())
    }
}

impl SpatialGeometryPublication {
    pub fn new(
        revision: u64,
        movement: Arc<CollisionIndex>,
        combat_baseline: Arc<CollisionIndex>,
        combat_replacements: BTreeMap<SectorKey, Arc<CollisionIndex>>,
    ) -> Self {
        Self {
            revision,
            movement,
            combat_baseline,
            combat_replacements,
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn movement(&self) -> &Arc<CollisionIndex> {
        &self.movement
    }

    pub fn combat_baseline(&self) -> &Arc<CollisionIndex> {
        &self.combat_baseline
    }

    pub fn combat_replacements(&self) -> &BTreeMap<SectorKey, Arc<CollisionIndex>> {
        &self.combat_replacements
    }
}

/// Server-certified coverage for one exact shot ray and one publication.
/// Simulation still rejects a plan whose ray or geometry revision differs.
#[derive(Clone)]
pub struct CombatCoveragePlan {
    pub(crate) publication: Arc<SpatialGeometryPublication>,
    pub(crate) origin: [f32; 3],
    pub(crate) direction: [f32; 3],
    pub(crate) range: f32,
    pub(crate) domains: Vec<CombatRaySectorInterval>,
    pub(crate) ready: bool,
}

impl CombatCoveragePlan {
    pub(crate) fn new(
        publication: Arc<SpatialGeometryPublication>,
        origin: [f32; 3],
        direction: [f32; 3],
        range: f32,
        domains: Vec<CombatRaySectorInterval>,
        ready: bool,
    ) -> Self {
        let length = direction
            .iter()
            .map(|value| value * value)
            .sum::<f32>()
            .sqrt();
        let direction = if length.is_finite() && length > 1.0e-8 {
            let inverse = length.recip();
            direction.map(|value| value * inverse)
        } else {
            direction
        };
        Self {
            publication,
            origin,
            direction,
            range,
            domains,
            ready,
        }
    }

    pub(crate) fn matches(
        &self,
        publication: &Arc<SpatialGeometryPublication>,
        origin: [f32; 3],
        direction: [f32; 3],
        range: f32,
    ) -> bool {
        Arc::ptr_eq(&self.publication, publication)
            && self.origin == origin
            && self.direction == direction
            && self.range == range
            && self.ready
    }
}
