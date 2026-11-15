//! Immutable material depths bound to one recognized world content identity.
use minimium_world::world::WorldManifest;

const MAX_SOIL_DEPTH_MM: u16 = 16_000;

/// Canonical terrain material policy supplied by recognized game content.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TerrainMaterialProfile {
    soil_depth_mm: u16,
}

impl TerrainMaterialProfile {
    pub fn new(soil_depth_mm: u16) -> Option<Self> {
        (soil_depth_mm <= MAX_SOIL_DEPTH_MM).then_some(Self { soil_depth_mm })
    }

    pub const fn soil_depth_mm(self) -> u16 {
        self.soil_depth_mm
    }

    pub fn bind(
        self,
        manifest: WorldManifest,
        map_fingerprint: u64,
    ) -> ResolvedTerrainMaterialProfile {
        ResolvedTerrainMaterialProfile {
            profile: self,
            manifest,
            map_fingerprint,
        }
    }
}

/// A material profile selected while resolving a specific immutable map.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ResolvedTerrainMaterialProfile {
    profile: TerrainMaterialProfile,
    manifest: WorldManifest,
    map_fingerprint: u64,
}

impl ResolvedTerrainMaterialProfile {
    pub const fn profile(self) -> TerrainMaterialProfile {
        self.profile
    }

    pub const fn manifest(self) -> WorldManifest {
        self.manifest
    }

    pub fn matches(self, manifest: WorldManifest, map_fingerprint: u64) -> bool {
        self.manifest == manifest && self.map_fingerprint == map_fingerprint
    }

    pub fn matches_manifest(self, manifest: WorldManifest) -> bool {
        self.manifest == manifest
    }
}
