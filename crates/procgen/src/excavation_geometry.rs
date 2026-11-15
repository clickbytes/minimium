//! Checked conversion of committed voxel chunks into shared sector geometry.
use crate::{
    planet_sector_count, sector_clip, terrain_seam::replace_chunk_domain, ExcavationGeometryError,
    PlanetTerrain, SectorGeometry,
};
use minimium_terrain::{TerrainMesh, TerrainStore, TerrainVertex};
use minimium_world::{
    terrain::{TerrainChunkKey, TerrainMaterial, TERRAIN_CHUNK_M},
    world::{
        MapSurfaceKind, MapSurfaceVertex, MapTriangle, PlanetSectorKey, SectorKey, WorldManifest,
    },
};

const MAX_VOLUME_CONTOUR_SECTORS: usize = 128;

/// The exact voxel keys used by both MC replacement and overlay invalidation.
pub fn volume_chunk_keys_near_region(
    store: &TerrainStore,
    face: minimium_world::world::PlanetFace,
    u: u16,
    v: u16,
    span: u16,
    count: u16,
) -> Vec<TerrainChunkKey> {
    if span == 0 || count == 0 {
        return Vec::new();
    }
    let mut keys: Vec<_> = store
        .chunks()
        .map(|chunk| chunk.key())
        .filter(|key| chunk_near_region(*key, face, u, v, span, count))
        .collect();
    let core = keys.clone();
    for key in core {
        for axis in 0..3 {
            for side in [-1, 1] {
                let mut adjacent = key.coordinates;
                adjacent[axis] += side;
                let neighbor = TerrainChunkKey {
                    world: key.world,
                    coordinates: adjacent,
                };
                if store.chunk(neighbor).is_some() && !keys.contains(&neighbor) {
                    keys.push(neighbor);
                }
            }
        }
    }
    keys.sort_unstable();
    keys
}

/// Base sectors that can contribute contours on the selected voxel domains.
/// For a positive face denominator, each projected UV coordinate is linear
/// fractional over the box, so its extrema occur at one of the eight corners.
pub fn volume_contour_sectors(
    store: &TerrainStore,
    face: minimium_world::world::PlanetFace,
    u: u16,
    v: u16,
    span: u16,
    count: u16,
) -> Result<Vec<PlanetSectorKey>, ExcavationGeometryError> {
    let mut sectors = std::collections::BTreeSet::new();
    let keys = volume_chunk_keys_near_region(store, face, u, v, span, count);
    for key in &keys {
        let origin = key.origin_m();
        let corners: [[f64; 3]; 8] = std::array::from_fn(|corner| {
            std::array::from_fn(|axis| {
                origin[axis]
                    + if corner & (1 << axis) == 0 {
                        0.0
                    } else {
                        TERRAIN_CHUNK_M
                    }
            })
        });
        let mut projected = false;
        for candidate_face in minimium_world::world::PlanetFace::ALL {
            let (normal, u_axis, v_axis) = face_basis(candidate_face);
            let normal_values = corners.map(|point| dot_axes(point, normal));
            let u_values = corners.map(|point| dot_axes(point, u_axis));
            let v_values = corners.map(|point| dot_axes(point, v_axis));
            let max_normal = normal_values
                .iter()
                .copied()
                .fold(f64::NEG_INFINITY, f64::max);
            let min_normal = normal_values.iter().copied().fold(f64::INFINITY, f64::min);
            let min_abs = |values: &[f64; 8]| {
                let minimum = values.iter().copied().fold(f64::INFINITY, f64::min);
                let maximum = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                if minimum <= 0.0 && maximum >= 0.0 {
                    0.0
                } else {
                    minimum.abs().min(maximum.abs())
                }
            };
            if max_normal <= 0.0
                || max_normal < min_abs(&u_values)
                || max_normal < min_abs(&v_values)
            {
                continue;
            }
            if min_normal <= 0.0 {
                return Err(ExcavationGeometryError::UnavailableSurface);
            }
            projected = true;
            let u_ratio =
                std::array::from_fn::<_, 8, _>(|index| u_values[index] / normal_values[index]);
            let v_ratio =
                std::array::from_fn::<_, 8, _>(|index| v_values[index] / normal_values[index]);
            let u_min = u_ratio
                .iter()
                .copied()
                .fold(f64::INFINITY, f64::min)
                .clamp(-1.0, 1.0);
            let u_max = u_ratio
                .iter()
                .copied()
                .fold(f64::NEG_INFINITY, f64::max)
                .clamp(-1.0, 1.0);
            let v_min = v_ratio
                .iter()
                .copied()
                .fold(f64::INFINITY, f64::min)
                .clamp(-1.0, 1.0);
            let v_max = v_ratio
                .iter()
                .copied()
                .fold(f64::NEG_INFINITY, f64::max)
                .clamp(-1.0, 1.0);
            if u_min > u_max || v_min > v_max {
                continue;
            }
            let coordinate = |ratio: f64| {
                ((((ratio + 1.0) * 0.5 * f64::from(count)).floor() as i64)
                    .clamp(0, i64::from(count - 1))) as u16
            };
            let lo_u = coordinate(u_min).saturating_sub(1);
            let hi_u = coordinate(u_max).saturating_add(1).min(count - 1);
            let lo_v = coordinate(v_min).saturating_sub(1);
            let hi_v = coordinate(v_max).saturating_add(1).min(count - 1);
            for sector_u in lo_u..=hi_u {
                for sector_v in lo_v..=hi_v {
                    let Some(sector) = PlanetSectorKey::new(candidate_face, sector_u, sector_v)
                    else {
                        continue;
                    };
                    sectors.insert(sector);
                    if sectors.len() > MAX_VOLUME_CONTOUR_SECTORS {
                        return Err(ExcavationGeometryError::UnavailableSurface);
                    }
                }
            }
        }
        if !projected {
            return Err(ExcavationGeometryError::UnavailableSurface);
        }
    }
    if sectors.is_empty() && !keys.is_empty() {
        return Err(ExcavationGeometryError::UnavailableSurface);
    }
    Ok(sectors.into_iter().collect())
}

/// Replaces the old terrain surface with voxel Marching Cubes and checked zipper
/// strips for the chunks projected into this sector's guarded neighborhood.
pub fn integrate_volume_sector(
    terrain: &PlanetTerrain,
    sector: PlanetSectorKey,
    count: u16,
    base_geometry: &SectorGeometry,
    soil_depth_m: f64,
) -> Result<SectorGeometry, ExcavationGeometryError> {
    let (u, v) = sector.coordinates();
    integrate_volume_region_with_contours(
        terrain,
        sector.face(),
        u,
        v,
        1,
        count,
        base_geometry,
        &base_geometry.triangles,
        soil_depth_m,
    )
}

pub fn integrate_volume_sector_with_contours(
    terrain: &PlanetTerrain,
    sector: PlanetSectorKey,
    count: u16,
    base_geometry: &SectorGeometry,
    contour_source: &[minimium_world::world::MapTriangle],
    soil_depth_m: f64,
) -> Result<SectorGeometry, ExcavationGeometryError> {
    let (u, v) = sector.coordinates();
    integrate_volume_region_with_contours(
        terrain,
        sector.face(),
        u,
        v,
        1,
        count,
        base_geometry,
        contour_source,
        soil_depth_m,
    )
}

pub fn integrate_volume_region_with_contours(
    terrain: &PlanetTerrain,
    face: minimium_world::world::PlanetFace,
    u: u16,
    v: u16,
    span: u16,
    count: u16,
    base_geometry: &SectorGeometry,
    contour_source: &[minimium_world::world::MapTriangle],
    soil_depth_m: f64,
) -> Result<SectorGeometry, ExcavationGeometryError> {
    let Some(store) = terrain.inputs().volumes.as_deref() else {
        return Ok(base_geometry.clone());
    };
    let Some(base) = terrain.volume_base(soil_depth_m) else {
        return Err(ExcavationGeometryError::UnavailableSurface);
    };
    let mut keys = volume_chunk_keys_near_region(store, face, u, v, span, count);
    if keys.is_empty() {
        return Ok(base_geometry.clone());
    }
    keys.sort_unstable();
    let view = store.view(&base);
    let mut voxel = Vec::new();
    for key in &keys {
        let mesh = view
            .mesh(*key)
            .map_err(|_| ExcavationGeometryError::UnavailableSurface)?;
        voxel.extend(mesh_triangles(mesh));
    }
    let mut prepared = replace_chunk_domain(
        base_geometry,
        contour_source,
        &voxel,
        &keys,
        |point| planet_surface(terrain, point),
        |point| voxel_surface(&view, point),
    )?;
    prepared.triangles =
        sector_clip::clip_triangles_region(&prepared.triangles, face, u, v, span, count);
    Ok(prepared)
}

/// Preflight all sectors whose chunk footprints touch the edit. The returned
/// geometries are immutable candidates; callers install them only after durable
/// chunk publication and reject the edit if this function returns an error.
pub fn validate_volume_geometry(
    map: &minimium_world::world::WorldMap,
    manifest: WorldManifest,
    terrain: &PlanetTerrain,
    changed: &[TerrainChunkKey],
    soil_depth_m: f64,
) -> Result<Vec<(SectorKey, SectorGeometry)>, ExcavationGeometryError> {
    let count = planet_sector_count(manifest);
    let mut sectors = std::collections::BTreeSet::new();
    for key in changed {
        for corner in 0..8 {
            let point = std::array::from_fn(|axis| {
                f64::from(key.coordinates[axis]) * f64::from(TERRAIN_CHUNK_M)
                    + if corner & (1 << axis) == 0 {
                        0.0
                    } else {
                        f64::from(TERRAIN_CHUNK_M)
                    }
            });
            let length = point.iter().map(|v| v * v).sum::<f64>().sqrt();
            if length < 1.0 {
                continue;
            }
            let direction = point.map(|v| (v / length) as f32);
            let sector = PlanetSectorKey::from_direction_with_count(direction, count);
            sectors.insert(sector);
            sectors.extend(sector.neighbors_with_count(count));
        }
    }
    let mut prepared = Vec::with_capacity(sectors.len());
    for sector in sectors {
        let key = SectorKey::Planet(sector);
        let geometry = crate::materialize_world_sector_with_geography_and_soil(
            map,
            manifest,
            key,
            terrain.inputs().clone(),
            soil_depth_m,
        )?;
        prepared.push((key, geometry));
    }
    Ok(prepared)
}

fn chunk_near_region(
    key: TerrainChunkKey,
    face: minimium_world::world::PlanetFace,
    u: u16,
    v: u16,
    span: u16,
    count: u16,
) -> bool {
    let (normal, u_axis, v_axis) = face_basis(face);
    let u_min = -1.0 + 2.0 * f64::from(u) / f64::from(count) - 2.0 / f64::from(count);
    let u_max = -1.0 + 2.0 * f64::from(u + span) / f64::from(count) + 2.0 / f64::from(count);
    let v_min = -1.0 + 2.0 * f64::from(v) / f64::from(count) - 2.0 / f64::from(count);
    let v_max = -1.0 + 2.0 * f64::from(v + span) / f64::from(count) + 2.0 / f64::from(count);
    let origin = key.origin_m();
    let corners: [[f64; 3]; 8] = std::array::from_fn(|corner| {
        std::array::from_fn(|axis| {
            origin[axis]
                + if corner & (1 << axis) == 0 {
                    0.0
                } else {
                    TERRAIN_CHUNK_M
                }
        })
    });
    let normal_values = corners.map(|point| dot_axes(point, normal));
    if normal_values
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max)
        <= 0.0
    {
        return false;
    }
    // These independent half-space maxima conservatively detect intersection
    // of the projected AABB with the region rectangle, including interior hits
    // where none of the AABB corners lies inside the rectangle.
    let linear_max = |axis_a: [i8; 3], scale_a: f64, axis_b: [i8; 3], scale_b: f64| {
        corners
            .iter()
            .map(|point| scale_a * dot_axes(*point, axis_a) + scale_b * dot_axes(*point, axis_b))
            .fold(f64::NEG_INFINITY, f64::max)
    };
    let possible = |axis: [i8; 3], lo: f64, hi: f64| {
        linear_max(axis, 1.0, normal, -lo) >= 0.0 && linear_max(normal, hi, axis, -1.0) >= 0.0
    };
    let u_possible = possible(u_axis, u_min, u_max);
    let v_possible = possible(v_axis, v_min, v_max);
    u_possible && v_possible
}

fn face_basis(face: minimium_world::world::PlanetFace) -> ([i8; 3], [i8; 3], [i8; 3]) {
    use minimium_world::world::PlanetFace::*;
    match face {
        PositiveX => ([1, 0, 0], [0, 1, 0], [0, 0, 1]),
        NegativeX => ([-1, 0, 0], [0, 1, 0], [0, 0, 1]),
        PositiveY => ([0, 1, 0], [1, 0, 0], [0, 0, 1]),
        NegativeY => ([0, -1, 0], [1, 0, 0], [0, 0, 1]),
        PositiveZ => ([0, 0, 1], [1, 0, 0], [0, 1, 0]),
        NegativeZ => ([0, 0, -1], [1, 0, 0], [0, 1, 0]),
    }
}

fn dot_axes(a: [f64; 3], b: [i8; 3]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * f64::from(y)).sum()
}

fn mesh_triangles(mesh: TerrainMesh) -> Vec<MapTriangle> {
    mesh.triangles
        .into_iter()
        .map(|vertices| {
            let points = vertices.map(|v| v.position_m.map(|c| (c * 1000.0).round() as i64));
            let surface = vertices.map(volume_payload);
            let material = vertices[0].material;
            MapTriangle {
                vertices: points.map(|p| p.map(|c| c as f32 / 1000.0)),
                precise_vertices: Some(points),
                color: material_color(material),
                solid: true,
                surface: Some(surface),
            }
        })
        .collect()
}

fn volume_payload(vertex: TerrainVertex) -> MapSurfaceVertex {
    let mut coverage = [0.0; 6];
    coverage[match vertex.material {
        TerrainMaterial::Soil => 1,
        TerrainMaterial::Rock => 4,
        TerrainMaterial::Empty => 1,
    }] = 1.0;
    MapSurfaceVertex {
        kind: MapSurfaceKind::Excavation,
        normal: vertex.normal.map(|v| v as f32),
        coverage,
        source_elevation_m: 0.0,
        surface_elevation_m: 0.0,
        slope: 0.0,
        water: 0.0,
        habitat: [0.0; 3],
    }
}

fn planet_surface(terrain: &PlanetTerrain, point: [i64; 3]) -> Option<MapSurfaceVertex> {
    let metres = point.map(|v| v as f64 / 1000.0);
    let radius = metres.iter().map(|v| v * v).sum::<f64>().sqrt();
    if radius <= 1.0 {
        return None;
    }
    let sample = terrain.surface_sample(metres.map(|v| v / radius)).ok()?;
    Some(MapSurfaceVertex {
        kind: MapSurfaceKind::PlanetGround,
        normal: sample.normal,
        coverage: sample.coverage,
        source_elevation_m: sample.source_elevation_m,
        surface_elevation_m: sample.elevation_m,
        slope: sample.slope,
        water: sample.water,
        habitat: [sample.landform.retention, sample.landform.exposure, 0.0],
    })
}

fn voxel_surface(
    view: &minimium_terrain::TerrainView<'_>,
    point: [i64; 3],
) -> Option<MapSurfaceVertex> {
    let metres = point.map(|v| v as f64 / 1000.0);
    let sample = view.interpolated(metres)?;
    let mut result = volume_payload(TerrainVertex {
        position_m: metres,
        normal: view.normal(metres)?,
        material: sample.material,
    });
    result.source_elevation_m = sample.distance_m as f32;
    Some(result)
}

fn material_color(material: TerrainMaterial) -> [f32; 3] {
    match material {
        TerrainMaterial::Soil => [0.34, 0.22, 0.12],
        TerrainMaterial::Rock => [0.28, 0.29, 0.30],
        TerrainMaterial::Empty => [0.34, 0.22, 0.12],
    }
}
