//! Explicit immutable inputs; geographic datasets never carry mutable world state.
use crate::PreparedGeography;
use minimium_terrain::{TerrainSnapshot, TerrainStore};
use minimium_world::{remnants::RemnantSnapshot, world::SectorKey};
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub struct TerrainInputs {
    pub geography: Option<Arc<dyn PreparedGeography>>,
    pub edits: Option<Arc<TerrainSnapshot>>,
    pub remnants: Option<Arc<RemnantSnapshot>>,
    pub volumes: Option<Arc<TerrainStore>>,
}

impl From<Option<Arc<dyn PreparedGeography>>> for TerrainInputs {
    fn from(geography: Option<Arc<dyn PreparedGeography>>) -> Self {
        Self {
            geography,
            ..Self::default()
        }
    }
}

impl TerrainInputs {
    pub fn revision(&self) -> (u64, u64, u64) {
        (
            self.edits.as_ref().map_or(0, |t| t.revision()),
            self.remnants.as_ref().map_or(0, |r| r.revision()),
            self.volumes.as_ref().map_or(0, |v| v.last_mutation()),
        )
    }

    pub fn sector_revision(&self, key: SectorKey, span: u16, count: u16) -> u64 {
        let edits = self
            .edits
            .as_ref()
            .map_or(0, |t| t.sector_revision(key, span, count));
        let remnants = self
            .remnants
            .as_ref()
            .map_or(0, |r| r.sector_revision(key, span, count));
        edits ^ remnants ^ self.volume_sector_revision(key, span, count)
    }

    /// Active chunks whose projected footprints and one-sector halo overlap
    /// this sector. Renderers use this to suppress the baseline ocean locally.
    pub fn volume_chunks_near_sector(
        &self,
        key: SectorKey,
        span: u16,
        count: u16,
    ) -> Vec<minimium_world::terrain::TerrainChunkKey> {
        let (Some(store), SectorKey::Planet(sector)) = (&self.volumes, key) else {
            return Vec::new();
        };
        if count == 0 {
            return Vec::new();
        }
        let neighbors = sector.neighbors_with_count(count);
        store
            .chunks()
            .filter_map(|chunk| {
                let coordinates = chunk.key().coordinates;
                let nearby = (0..8).any(|corner| {
                    let point = std::array::from_fn(|axis| {
                        (f64::from(coordinates[axis]) * 32.0
                            + if corner & (1 << axis) == 0 { 0.0 } else { 32.0 })
                            * 0.5
                    });
                    let length = point.iter().map(|v| v * v).sum::<f64>().sqrt();
                    if length < 1.0 {
                        return true;
                    }
                    let candidate =
                        minimium_world::world::PlanetSectorKey::from_direction_with_count(
                            point.map(|v| (v / length) as f32),
                            count,
                        );
                    if candidate == sector || neighbors.contains(&candidate) {
                        return true;
                    }
                    if span > 1 && candidate.face() == sector.face() {
                        let (u, v) = sector.coordinates();
                        let (cu, cv) = candidate.coordinates();
                        cu.abs_diff(u) <= span && cv.abs_diff(v) <= span
                    } else {
                        false
                    }
                });
                nearby.then_some(chunk.key())
            })
            .collect()
    }

    fn volume_sector_revision(&self, key: SectorKey, span: u16, count: u16) -> u64 {
        let Some(store) = &self.volumes else {
            return 0;
        };
        let mut revision = 0;
        for key in self.volume_chunks_near_sector(key, span, count) {
            if let Some(chunk) = store.chunk(key) {
                let key_hash = key.coordinates.iter().fold(0x9e3779b97f4a7c15_u64, |h, c| {
                    h.rotate_left(13) ^ (*c as u64).wrapping_mul(0xbf58476d1ce4e5b9)
                });
                revision ^= key_hash ^ chunk.revision().0.rotate_left(29);
            }
        }
        revision
    }
}
