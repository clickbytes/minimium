//! Immutable resolved terrain identity and independently revisioned overlays.
use crate::PreparedGeography;
use minimium_terrain::{TerrainSnapshot, TerrainStore};
use minimium_world::{
    remnants::RemnantSnapshot,
    world::{SectorKey, WorldInstanceId, WorldManifest, WorldMap, WorldSource, WorldTopology},
};
use std::sync::Arc;

/// Independent monotone revisions for mutable inputs affecting terrain geometry.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct SectorContentVersion {
    pub edits: u64,
    pub remnants: u64,
    pub volume: u64,
}

/// Validated immutable source for one manifest and exact map fingerprint.
#[derive(Clone, Debug)]
pub struct PreparedTerrainSource {
    world: WorldInstanceId,
    manifest: WorldManifest,
    map_fingerprint: u64,
    geography: Option<Arc<dyn PreparedGeography>>,
    material_profile: crate::ResolvedTerrainMaterialProfile,
    relief_policy: Option<crate::ResolvedPlanetReliefPolicy>,
}

impl PreparedTerrainSource {
    pub fn new(
        world: WorldInstanceId,
        manifest: WorldManifest,
        map: &WorldMap,
        geography: Option<Arc<dyn PreparedGeography>>,
        material_profile: crate::ResolvedTerrainMaterialProfile,
        relief_policy: Option<crate::ResolvedPlanetReliefPolicy>,
    ) -> Result<Self, crate::GenerationError> {
        crate::validate_world_manifest(manifest)?;
        let fingerprint = map.fingerprint();
        if !material_profile.matches(manifest, fingerprint) {
            return Err(crate::GenerationError::invalid_manifest(
                "terrain material profile does not match the world manifest and map",
            ));
        }
        match relief_policy {
            Some(policy) if !policy.matches(manifest, fingerprint) => {
                return Err(crate::GenerationError::invalid_manifest(
                    "terrain relief policy does not match the world manifest and map",
                ));
            }
            None if manifest.source() == WorldSource::Generated
                && manifest.topology() == WorldTopology::Planet =>
            {
                return Err(crate::GenerationError::invalid_manifest(
                    "generated planet has no resolved relief sampling policy",
                ));
            }
            _ => {}
        }
        if let Some(definition) = manifest.planet().filter(|planet| planet.mode == 1) {
            let geography = geography.as_ref().ok_or_else(|| {
                crate::GenerationError::sampling(crate::EarthSampleError::PackageUnavailable)
            })?;
            if geography.package_digest() != definition.package_digest {
                return Err(crate::GenerationError::sampling(
                    crate::EarthSampleError::PackageMismatch,
                ));
            }
        }
        Ok(Self {
            world,
            manifest,
            map_fingerprint: fingerprint,
            geography,
            material_profile,
            relief_policy,
        })
    }

    pub const fn world(&self) -> WorldInstanceId {
        self.world
    }
    pub const fn manifest(&self) -> WorldManifest {
        self.manifest
    }
    pub const fn map_fingerprint(&self) -> u64 {
        self.map_fingerprint
    }
    pub fn geography(&self) -> Option<&Arc<dyn PreparedGeography>> {
        self.geography.as_ref()
    }
    pub const fn material_profile(&self) -> crate::ResolvedTerrainMaterialProfile {
        self.material_profile
    }
    pub const fn relief_policy(&self) -> Option<crate::ResolvedPlanetReliefPolicy> {
        self.relief_policy
    }

    pub fn matches_map(
        &self,
        world: WorldInstanceId,
        manifest: WorldManifest,
        map: &WorldMap,
    ) -> bool {
        self.world == world
            && self.manifest == manifest
            && self.map_fingerprint == map.fingerprint()
    }
}

/// Mutable world overlays captured as one immutable worker snapshot.
#[derive(Clone, Debug, Default)]
pub struct TerrainOverlaySnapshot {
    world: Option<WorldInstanceId>,
    edits: Option<Arc<TerrainSnapshot>>,
    remnants: Option<Arc<RemnantSnapshot>>,
    volumes: Option<Arc<TerrainStore>>,
}

impl TerrainOverlaySnapshot {
    pub fn new(
        world: WorldInstanceId,
        edits: Option<Arc<TerrainSnapshot>>,
        remnants: Option<Arc<RemnantSnapshot>>,
        volumes: Option<Arc<TerrainStore>>,
    ) -> Self {
        Self {
            world: Some(world),
            edits,
            remnants,
            volumes,
        }
    }

    pub fn empty_for(world: WorldInstanceId) -> Self {
        Self {
            world: Some(world),
            ..Self::default()
        }
    }

    pub const fn world(&self) -> Option<WorldInstanceId> {
        self.world
    }

    pub fn edits(&self) -> Option<&Arc<TerrainSnapshot>> {
        self.edits.as_ref()
    }
    pub fn remnants(&self) -> Option<&Arc<RemnantSnapshot>> {
        self.remnants.as_ref()
    }
    pub fn volumes(&self) -> Option<&Arc<TerrainStore>> {
        self.volumes.as_ref()
    }

    pub fn revision(&self) -> SectorContentVersion {
        SectorContentVersion {
            edits: self.edits.as_ref().map_or(0, |terrain| terrain.revision()),
            remnants: self
                .remnants
                .as_ref()
                .map_or(0, |remnants| remnants.revision()),
            volume: self
                .volumes
                .as_ref()
                .map_or(0, |volumes| volumes.last_mutation()),
        }
    }

    pub fn sector_version(&self, _key: SectorKey, _span: u16, _count: u16) -> SectorContentVersion {
        self.revision()
    }

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

/// Complete immutable worker input: prepared source plus world-local overlays.
#[derive(Clone, Debug)]
pub struct TerrainInputs {
    pub(crate) source: PreparedTerrainSource,
    pub(crate) overlays: TerrainOverlaySnapshot,
}

impl TerrainInputs {
    pub fn new(
        source: PreparedTerrainSource,
        overlays: TerrainOverlaySnapshot,
    ) -> Result<Self, crate::GenerationError> {
        if overlays.world != Some(source.world) {
            return Err(crate::GenerationError::invalid_manifest(
                "terrain overlays belong to a different world instance",
            ));
        }
        if overlays
            .volumes
            .as_ref()
            .is_some_and(|store| store.world() != source.world)
        {
            return Err(crate::GenerationError::invalid_manifest(
                "terrain volume overlay belongs to a different world instance",
            ));
        }
        Ok(Self { source, overlays })
    }
    pub fn source(&self) -> &PreparedTerrainSource {
        &self.source
    }
    pub fn overlays(&self) -> &TerrainOverlaySnapshot {
        &self.overlays
    }
    pub fn revision(&self) -> SectorContentVersion {
        self.overlays.revision()
    }
    pub fn sector_version(&self, key: SectorKey, span: u16, count: u16) -> SectorContentVersion {
        self.overlays.sector_version(key, span, count)
    }
    pub fn volume_chunks_near_sector(
        &self,
        key: SectorKey,
        span: u16,
        count: u16,
    ) -> Vec<minimium_world::terrain::TerrainChunkKey> {
        self.overlays.volume_chunks_near_sector(key, span, count)
    }
}

/// Explicit geography and policy for generation before a map fingerprint exists.
#[derive(Clone, Debug)]
pub struct PlanetGenerationSource {
    geography: Option<Arc<dyn PreparedGeography>>,
}

impl PlanetGenerationSource {
    pub(crate) fn from_prepared(source: &PreparedTerrainSource) -> Self {
        Self {
            geography: source.geography().cloned(),
        }
    }

    pub fn new(
        definition: Option<minimium_world::world::PlanetDefinition>,
        geography: Option<Arc<dyn PreparedGeography>>,
    ) -> Result<Self, crate::GenerationError> {
        if let Some(definition) = definition.filter(|planet| planet.mode == 1) {
            let geography = geography.as_ref().ok_or_else(|| {
                crate::GenerationError::sampling(crate::EarthSampleError::PackageUnavailable)
            })?;
            if geography.package_digest() != definition.package_digest {
                return Err(crate::GenerationError::sampling(
                    crate::EarthSampleError::PackageMismatch,
                ));
            }
        }
        Ok(Self { geography })
    }

    pub fn geography(&self) -> Option<&Arc<dyn PreparedGeography>> {
        self.geography.as_ref()
    }
}
