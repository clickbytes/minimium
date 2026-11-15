//! World-space contracts shared by terrain, movement and adapters.
use crate::{entity::EntityId, world::WorldInstanceId};

pub const TERRAIN_CELL_M: f64 = 0.5;
pub const TERRAIN_CHUNK_CELLS: i32 = 32;
pub const TERRAIN_CHUNK_M: f64 = 16.0;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct TerrainRevision(pub u64);

impl TerrainRevision {
    pub fn next(self) -> Option<Self> {
        self.0.checked_add(1).map(Self)
    }
}

/// Integer lattice coordinates, independent of floating render origins.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct TerrainChunkKey {
    pub world: WorldInstanceId,
    pub coordinates: [i32; 3],
}

impl TerrainChunkKey {
    pub fn at(world: WorldInstanceId, position: [f64; 3]) -> Option<Self> {
        if !valid_position(position) {
            return None;
        }
        let key = Self {
            world,
            coordinates: position.map(|v| (v / TERRAIN_CHUNK_M).floor() as i32),
        };
        key.valid().then_some(key)
    }

    pub fn origin_m(self) -> [f64; 3] {
        self.coordinates.map(|v| f64::from(v) * TERRAIN_CHUNK_M)
    }

    pub fn valid(self) -> bool {
        self.coordinates
            .iter()
            .all(|v| (-625_000..625_000).contains(v))
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TerrainMaterial {
    #[default]
    Empty,
    Soil,
    Rock,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainSample {
    /// Positive in free space; negative inside material. Units: metres.
    pub distance_m: f64,
    pub material: TerrainMaterial,
    pub revision: TerrainRevision,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SurfaceTarget {
    Terrain(TerrainChunkKey),
    Entity(EntityId),
    StaticGeometry,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceHit {
    pub position_m: [f64; 3],
    pub normal: [f64; 3],
    pub distance_m: f64,
    pub target: SurfaceTarget,
    pub material: TerrainMaterial,
    pub revision: TerrainRevision,
}

/// Immutable spatial port. A missing sample is unavailable, never assumed empty.
pub trait TerrainRead: Send + Sync {
    fn sample(&self, position_m: [f64; 3]) -> Option<TerrainSample>;
    fn surface_hit(
        &self,
        origin_m: [f64; 3],
        direction: [f64; 3],
        reach_m: f64,
    ) -> Option<SurfaceHit>;
}

pub fn valid_position(position: [f64; 3]) -> bool {
    position
        .iter()
        .all(|v| v.is_finite() && v.abs() <= 10_000_000.0)
}
