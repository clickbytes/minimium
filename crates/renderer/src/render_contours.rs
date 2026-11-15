//! Bounded prepass that gives excavation seams the rendered base LOD over the
//! exact projected chunk footprint and its one-sector UV guard.
use crate::render_patch_cache::RenderPatchCache;
use minimium_procgen::SectorGeometry;
use minimium_world::world::{MapTriangle, PlanetSectorKey, SectorKey, WorldManifest};
use std::{
    collections::{BTreeSet, HashMap},
    hash::{Hash, Hasher},
    sync::Arc,
};

const MAX_GUARD_SECTORS: usize = 1_024;
const MAX_PREPASS_BYTES: usize = 96 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum RenderMeshKind {
    Patch,
    Block,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct RenderRegion {
    pub sector: PlanetSectorKey,
    pub span: u16,
    pub lod: u8,
}

pub(crate) struct PreparedRenderRegion {
    pub geometry: Arc<SectorGeometry>,
    contour_sources: Vec<RegionKey>,
    pub cache_revision: u64,
}

pub(crate) struct RenderContourPrepass {
    bases: HashMap<RegionKey, (u64, Arc<SectorGeometry>)>,
    sources: HashMap<RegionKey, Vec<RegionKey>>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct RegionKey {
    sector: PlanetSectorKey,
    span: u16,
    lod: u8,
}

pub(crate) fn prepare(
    manifest: WorldManifest,
    inputs: &minimium_procgen::TerrainInputs,
    regions: &[RenderRegion],
    kind: RenderMeshKind,
    cache: &mut RenderPatchCache,
) -> Result<RenderContourPrepass, minimium_procgen::ExcavationGeometryError> {
    let count = minimium_procgen::planet_sector_count(manifest);
    let mut selected = BTreeSet::new();
    for region in regions {
        if region.span == 0 || !selected.insert(key(*region)) {
            return Err(minimium_procgen::ExcavationGeometryError::InvalidChunk);
        }
    }
    let active: Vec<_> = regions
        .iter()
        .copied()
        .filter(|region| {
            !inputs
                .volume_chunks_near_sector(SectorKey::Planet(region.sector), region.span, count)
                .is_empty()
        })
        .collect();
    if active.is_empty() {
        return Ok(RenderContourPrepass {
            bases: HashMap::new(),
            sources: HashMap::new(),
        });
    }

    let mut coverage = HashMap::new();
    for region in &selected {
        for du in 0..region.span {
            for dv in 0..region.span {
                let (u, v) = region.sector.coordinates();
                if let Some(sector) = PlanetSectorKey::new(region.sector.face(), u + du, v + dv) {
                    coverage.insert(sector, *region);
                }
            }
        }
    }
    let guard_lod = match kind {
        RenderMeshKind::Patch => regions.iter().map(|region| region.lod).max().unwrap_or(16),
        RenderMeshKind::Block => 16,
    };
    let mut guards = BTreeSet::new();
    let mut sources = HashMap::new();
    let mut needed = BTreeSet::new();
    for region in &active {
        let target = key(*region);
        needed.insert(target);
        let mut target_sources = BTreeSet::from([target]);
        let contour_sectors = inputs
            .volumes
            .as_deref()
            .map(|store| {
                let (u, v) = region.sector.coordinates();
                minimium_procgen::volume_contour_sectors(
                    store,
                    region.sector.face(),
                    u,
                    v,
                    region.span,
                    count,
                )
            })
            .transpose()?
            .unwrap_or_default();
        for neighbor in contour_sectors {
            if let Some(source) = coverage.get(&neighbor) {
                target_sources.insert(*source);
                needed.insert(*source);
            } else {
                let guard = RegionKey {
                    sector: neighbor,
                    span: 1,
                    lod: guard_lod,
                };
                guards.insert(guard);
                needed.insert(guard);
                target_sources.insert(guard);
            }
        }
        sources.insert(target, target_sources.into_iter().collect());
    }
    if guards.len() > MAX_GUARD_SECTORS {
        return Err(minimium_procgen::ExcavationGeometryError::UnavailableSurface);
    }

    let mut bases = HashMap::with_capacity(needed.len());
    let mut bytes = 0usize;
    for region in &needed {
        let revision = inputs.sector_revision(SectorKey::Planet(region.sector), region.span, count);
        let geometry = cache.get_base_revision(
            SectorKey::Planet(region.sector),
            region.lod,
            revision,
            || generate_base(manifest, *region, kind, inputs.clone()),
        );
        bytes = bytes.saturating_add(geometry_bytes(&geometry));
        if bytes > MAX_PREPASS_BYTES {
            return Err(minimium_procgen::ExcavationGeometryError::UnavailableSurface);
        }
        bases.insert(*region, (revision, geometry));
    }
    let conform: Vec<_> = bases
        .iter()
        .map(|(region, (_, geometry))| {
            (
                SectorKey::Planet(region.sector),
                region.lod,
                geometry.clone(),
            )
        })
        .collect();
    let mut conform_terrain = minimium_procgen::PlanetTerrain::new(
        manifest.generation(),
        minimium_procgen::planet_surface_for(manifest),
    )
    .ok_or(minimium_procgen::ExcavationGeometryError::UnavailableSurface)?
    .with_inputs(inputs.clone());
    if let Some(definition) = manifest.planet() {
        conform_terrain = conform_terrain.with_definition(definition);
    }
    let conform = crate::terrain_edges::preconform_bases(conform, &conform_terrain);
    for (sector, lod, geometry) in conform {
        let SectorKey::Planet(sector) = sector else {
            continue;
        };
        if let Some((_, cached)) = bases
            .iter_mut()
            .find(|(region, _)| region.sector == sector && region.lod == lod)
        {
            cached.1 = geometry;
        }
    }
    Ok(RenderContourPrepass { bases, sources })
}

impl RenderContourPrepass {
    pub(crate) fn region(&self, region: RenderRegion) -> Option<PreparedRenderRegion> {
        let region_key = key(region);
        let (_, geometry) = self.bases.get(&region_key)?;
        let source_keys = self.sources.get(&region_key)?;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        region_key.hash(&mut hasher);
        for source in source_keys {
            source.hash(&mut hasher);
            self.bases[source].0.hash(&mut hasher);
        }
        let cache_revision = hasher.finish();
        Some(PreparedRenderRegion {
            geometry: geometry.clone(),
            contour_sources: source_keys.clone(),
            cache_revision,
        })
    }

    pub(crate) fn contours(&self, region: &PreparedRenderRegion) -> Vec<MapTriangle> {
        let count = region
            .contour_sources
            .iter()
            .map(|source| self.bases[source].1.triangles.len())
            .sum();
        let mut contours = Vec::with_capacity(count);
        for source in &region.contour_sources {
            contours.extend(self.bases[source].1.triangles.iter().copied());
        }
        contours
    }
}

fn geometry_bytes(geometry: &SectorGeometry) -> usize {
    (geometry.triangles.capacity() + geometry.object_triangles.capacity())
        .saturating_mul(std::mem::size_of::<MapTriangle>())
        .saturating_add(
            geometry
                .boxes
                .capacity()
                .saturating_mul(std::mem::size_of::<minimium_world::world::MapBox>()),
        )
}

fn key(region: RenderRegion) -> RegionKey {
    RegionKey {
        sector: region.sector,
        span: region.span,
        lod: region.lod,
    }
}

fn generate_base(
    manifest: WorldManifest,
    region: RegionKey,
    kind: RenderMeshKind,
    inputs: minimium_procgen::TerrainInputs,
) -> SectorGeometry {
    let (u, v) = region.sector.coordinates();
    match kind {
        RenderMeshKind::Patch => minimium_procgen::generate_planet_render_patch_with_geography(
            manifest,
            region.sector,
            u32::from(region.lod),
            inputs,
        ),
        RenderMeshKind::Block => minimium_procgen::generate_planet_render_block_with_geography(
            manifest,
            region.sector.face(),
            u,
            v,
            region.span,
            inputs,
        ),
    }
}
