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

/// Nonempty set of materials a deformation may affect. Empty space is never
/// a deformable material; game content selects the allowed solid materials.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct TerrainMaterialSet(u8);

impl TerrainMaterialSet {
    const SOIL_BIT: u8 = 1;
    const ROCK_BIT: u8 = 2;

    pub const SOIL: Self = Self(Self::SOIL_BIT);
    pub const ROCK: Self = Self(Self::ROCK_BIT);
    pub const SOIL_AND_ROCK: Self = Self(Self::SOIL_BIT | Self::ROCK_BIT);

    pub const fn new(soil: bool, rock: bool) -> Option<Self> {
        let bits = (soil as u8) * Self::SOIL_BIT | (rock as u8) * Self::ROCK_BIT;
        if bits == 0 { None } else { Some(Self(bits)) }
    }

    pub const fn contains(self, material: TerrainMaterial) -> bool {
        match material {
            TerrainMaterial::Empty => false,
            TerrainMaterial::Soil => self.0 & Self::SOIL_BIT != 0,
            TerrainMaterial::Rock => self.0 & Self::ROCK_BIT != 0,
        }
    }

    pub const fn valid(self) -> bool {
        self.0 != 0 && self.0 & !(Self::SOIL_BIT | Self::ROCK_BIT) == 0
    }
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
