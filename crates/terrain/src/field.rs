use minimium_world::{
    terrain::{
        valid_position, TerrainChunkKey, TerrainMaterial, TerrainRevision, TerrainSample,
        TERRAIN_CELL_M, TERRAIN_CHUNK_CELLS,
    },
    world::WorldInstanceId,
};
use std::{collections::BTreeMap, sync::Arc};

pub const SAMPLES_PER_CHUNK: usize = 32 * 32 * 32;

/// Immutable initial signed field, including any committed base deformation.
pub trait BaseTerrain: Send + Sync {
    fn sample(&self, position_m: [f64; 3]) -> Option<TerrainSample>;
}

#[derive(Clone, Copy, Debug)]
pub struct TerrainConfig {
    pub max_chunks: usize,
    pub max_edit_radius_m: f64,
}

impl TerrainConfig {
    pub fn valid(self) -> bool {
        (1..=65_536).contains(&self.max_chunks) && (0.25..=8.0).contains(&self.max_edit_radius_m)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChunkImage {
    pub(crate) key: TerrainChunkKey,
    pub(crate) revision: TerrainRevision,
    pub(crate) samples: Vec<TerrainSample>,
}

impl ChunkImage {
    pub fn from_samples(
        key: TerrainChunkKey,
        revision: TerrainRevision,
        samples: Vec<TerrainSample>,
    ) -> Option<Self> {
        if !key.valid()
            || samples.len() != SAMPLES_PER_CHUNK
            || revision.0 == 0
            || samples
                .iter()
                .any(|s| !s.distance_m.is_finite() || s.revision != revision)
        {
            return None;
        }
        Some(Self {
            key,
            revision,
            samples,
        })
    }

    pub fn key(&self) -> TerrainChunkKey {
        self.key
    }

    pub fn revision(&self) -> TerrainRevision {
        self.revision
    }

    pub fn samples(&self) -> &[TerrainSample] {
        &self.samples
    }
}

#[derive(Clone, Debug)]
pub struct TerrainStore {
    pub(crate) world: WorldInstanceId,
    pub(crate) config: TerrainConfig,
    pub(crate) chunks: BTreeMap<TerrainChunkKey, Arc<ChunkImage>>,
    pub(crate) last_mutation: u64,
}

impl TerrainStore {
    pub fn new(world: WorldInstanceId, config: TerrainConfig) -> Option<Self> {
        config.valid().then_some(Self {
            world,
            config,
            chunks: BTreeMap::new(),
            last_mutation: 0,
        })
    }

    pub fn view<'a>(&'a self, base: &'a dyn BaseTerrain) -> TerrainView<'a> {
        TerrainView { store: self, base }
    }

    pub fn chunk(&self, key: TerrainChunkKey) -> Option<&Arc<ChunkImage>> {
        self.chunks.get(&key)
    }

    pub fn chunks(&self) -> impl Iterator<Item = &Arc<ChunkImage>> {
        self.chunks.values()
    }

    pub fn last_mutation(&self) -> u64 {
        self.last_mutation
    }

    pub fn world(&self) -> WorldInstanceId {
        self.world
    }

    pub fn config(&self) -> TerrainConfig {
        self.config
    }

    /// Reconstructs a committed immutable snapshot from durable chunk images.
    pub fn from_chunks(
        world: WorldInstanceId,
        config: TerrainConfig,
        last_mutation: u64,
        chunks: impl IntoIterator<Item = Arc<ChunkImage>>,
    ) -> Option<Self> {
        let mut store = Self::new(world, config)?;
        store.last_mutation = last_mutation;
        for chunk in chunks {
            if chunk.key.world != world || store.chunks.insert(chunk.key, chunk).is_some() {
                return None;
            }
        }
        (store.chunks.len() <= config.max_chunks).then_some(store)
    }
}

pub struct TerrainView<'a> {
    pub(crate) store: &'a TerrainStore,
    pub(crate) base: &'a dyn BaseTerrain,
}

impl TerrainView<'_> {
    pub fn lattice_sample(&self, coordinate: [i32; 3]) -> Option<TerrainSample> {
        let key = TerrainChunkKey {
            world: self.store.world,
            coordinates: coordinate.map(|v| v.div_euclid(TERRAIN_CHUNK_CELLS)),
        };
        if let Some(chunk) = self.store.chunks.get(&key) {
            let [x, y, z] = coordinate.map(|v| v.rem_euclid(TERRAIN_CHUNK_CELLS) as usize);
            return Some(chunk.samples[x + 32 * (y + 32 * z)]);
        }
        let sample = self
            .base
            .sample(coordinate.map(|v| f64::from(v) * TERRAIN_CELL_M))?;
        sample.distance_m.is_finite().then_some(sample)
    }

    pub fn interpolated(&self, position: [f64; 3]) -> Option<TerrainSample> {
        if !valid_position(position) {
            return None;
        }
        let grid = position.map(|v| v / TERRAIN_CELL_M);
        let lower = grid.map(|v| v.floor() as i32);
        let fraction: [f64; 3] = std::array::from_fn(|i| grid[i] - f64::from(lower[i]));
        let mut result = TerrainSample {
            distance_m: 0.0,
            material: TerrainMaterial::Empty,
            revision: TerrainRevision(0),
        };
        let mut material_weight = -1.0;
        for corner in 0..8 {
            let coordinate = std::array::from_fn(|i| lower[i] + ((corner >> i) & 1));
            let weight: f64 = (0..3)
                .map(|i| {
                    if corner & (1 << i) == 0 {
                        1.0 - fraction[i]
                    } else {
                        fraction[i]
                    }
                })
                .product();
            let sample = self.lattice_sample(coordinate)?;
            result.distance_m += weight * sample.distance_m;
            result.revision = result.revision.max(sample.revision);
            if sample.material != TerrainMaterial::Empty && weight > material_weight {
                result.material = sample.material;
                material_weight = weight;
            }
        }
        Some(result)
    }

    pub fn normal(&self, point: [f64; 3]) -> Option<[f64; 3]> {
        let mut gradient = [0.0; 3];
        for axis in 0..3 {
            let mut a = point;
            let mut b = point;
            a[axis] -= 0.01;
            b[axis] += 0.01;
            gradient[axis] = self.interpolated(b)?.distance_m - self.interpolated(a)?.distance_m;
        }
        let length = gradient.iter().map(|v| v * v).sum::<f64>().sqrt();
        (length > 1.0e-12).then(|| gradient.map(|v| v / length))
    }
}
