//! Explicit immutable inputs; geographic datasets never carry mutable world state.
use crate::PreparedGeography;
use minimium_terrain::{TerrainSnapshot, TerrainStore};
use minimium_world::{remnants::RemnantSnapshot, world::SectorKey};
use std::sync::Arc;

/// Independent monotone revisions for the mutable terrain inputs that affect
/// generated sector and render geometry.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct SectorContentVersion {
    pub edits: u64,
    pub remnants: u64,
    pub volume: u64,
}

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
    pub fn revision(&self) -> SectorContentVersion {
        SectorContentVersion {
            edits: self.edits.as_ref().map_or(0, |t| t.revision()),
            remnants: self.remnants.as_ref().map_or(0, |r| r.revision()),
            volume: self.volumes.as_ref().map_or(0, |v| v.last_mutation()),
        }
    }

    /// Returns a conservative sector invalidation version. Each source is
    /// globally monotone, so unrelated changes may invalidate this sector.
    pub fn sector_version(&self, _key: SectorKey, _span: u16, _count: u16) -> SectorContentVersion {
        self.revision()
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
        let (u, v) = sector.coordinates();
        crate::volume_chunk_keys_near_region(store, sector.face(), u, v, span.max(1), count)
    }
}
