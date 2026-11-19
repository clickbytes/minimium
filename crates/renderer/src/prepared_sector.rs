//! CPU-only preparation suitable for a host worker; no device or queue access.
use crate::{
    geometry::{self, Vertex},
    shadow_geometry::ShadowBounds,
    terrain_geometry::TerrainVertex,
};
use minimium_procgen::{PlanetTerrain, SectorGeometry};
use minimium_world::world::SectorKey;
use std::sync::Arc;
#[derive(Clone)]
pub struct PreparedSector {
    pub key: SectorKey,
    pub lod: u8,
    pub geometry: Arc<SectorGeometry>,
    pub surface: Arc<Vec<TerrainVertex>>,
    pub ocean: Arc<Vec<crate::ocean_geometry::OceanVertex>>,
    pub objects: Arc<Vec<Vertex>>,
    pub shadow: Arc<Vec<Vertex>>,
    pub bounds: Option<ShadowBounds>,
    pub terrain: bool,
}
impl PreparedSector {
    pub fn new(
        key: SectorKey,
        lod: u8,
        geometry: Arc<SectorGeometry>,
        terrain: Option<&PlanetTerrain>,
        ocean: Option<&SectorGeometry>,
        origin: [f32; 3],
    ) -> Result<Self, minimium_procgen::GenerationError> {
        Self::new_with_clipped_ocean(key, lod, geometry, terrain, ocean, origin, None)
    }

    pub fn new_with_clipped_ocean(
        key: SectorKey,
        lod: u8,
        geometry: Arc<SectorGeometry>,
        terrain: Option<&PlanetTerrain>,
        ocean: Option<&SectorGeometry>,
        origin: [f32; 3],
        water: Option<(
            &minimium_procgen::WaterCatalog,
            &minimium_fluids::FluidSnapshot,
            &[[[f64; 3]; 2]],
        )>,
    ) -> Result<Self, minimium_procgen::GenerationError> {
        // Live reservoir patches retain the bed for a receding or absent surface.
        let visible = &geometry;
        let vertices = geometry::sector_geometry_relative(visible, origin);
        let objects = if terrain.is_some() {
            let mut objects = vertices[..geometry.boxes.len() * 36].to_vec();
            objects.extend_from_slice(
                &vertices[vertices.len() - visible.object_triangles.len() * 3..],
            );
            objects
        } else {
            Vec::new()
        };
        let surface = match terrain {
            Some(terrain) => crate::terrain_geometry::vertices(visible, terrain, origin)?,
            None => Vec::new(),
        };
        let shadow = if terrain.is_some() {
            let object_vertices =
                vertices[vertices.len() - visible.object_triangles.len() * 3..].to_vec();
            let mut shadow = crate::terrain_geometry::shadow_vertices(visible, vertices);
            shadow.extend(object_vertices);
            shadow
        } else {
            vertices
        };
        let bounds = crate::shadow_geometry::geometry_bounds(visible)
            .into_iter()
            .chain(ocean.and_then(crate::shadow_geometry::geometry_bounds))
            .reduce(|a, b| ShadowBounds {
                minimum: std::array::from_fn(|i| a.minimum[i].min(b.minimum[i])),
                maximum: std::array::from_fn(|i| a.maximum[i].max(b.maximum[i])),
            });
        Ok(Self {
            key,
            lod,
            geometry,
            surface: Arc::new(surface),
            ocean: Arc::new(
                ocean.map_or_else(Vec::new, |g| clipped_ocean_vertices(g, origin, water)),
            ),
            objects: Arc::new(objects),
            shadow: Arc::new(shadow),
            bounds,
            terrain: terrain.is_some(),
        })
    }
}

pub fn clipped_ocean_vertices(
    geometry: &SectorGeometry,
    origin: [f32; 3],
    water: Option<(
        &minimium_procgen::WaterCatalog,
        &minimium_fluids::FluidSnapshot,
        &[[[f64; 3]; 2]],
    )>,
) -> Vec<crate::ocean_geometry::OceanVertex> {
    use minimium_procgen::terrain_clip::{subtract_boxes, Box3};

    let Some((catalog, snapshot, interfaces)) = water else {
        return crate::ocean_geometry::vertices(geometry, origin);
    };
    let reference_radius = catalog.reference_sea_radius_m();
    let origin = glam::DVec3::from_array(origin.map(f64::from));
    let mut samples = std::collections::HashMap::new();
    let mut vertices = Vec::new();
    for triangle in &geometry.triangles {
        let points = triangle.precise_vertices.map_or_else(
            || triangle.vertices.map(|point| point.map(f64::from)),
            |points| points.map(|point| point.map(|value| value as f64 / 1000.0)),
        );
        let corners = points.map(|p| {
            *samples.entry(p.map(f64::to_bits)).or_insert_with(|| {
                let base = glam::DVec3::from_array(p);
                let direction = base.normalize_or_zero().to_array();
                let level = catalog
                    .body_at_direction(direction)
                    .and_then(|id| snapshot.reservoirs().get(&id))
                    .and_then(|body| body.level_m().ok());
                let depth_m = level
                    .and_then(|level| {
                        Some(level - catalog.ground_radius_at_direction_m(direction)?)
                    })
                    .unwrap_or(-1.0);
                WaterSurfacePoint {
                    base,
                    scale: level.map(|level| level / reference_radius),
                    depth_m,
                }
            })
        });
        if corners.iter().all(|corner| corner.depth_m <= 0.0) {
            continue;
        }
        let surface = wet_surface(corners);
        let boxes: Vec<_> = interfaces
            .iter()
            .filter(|[lo, hi]| {
                (0..3).all(|axis| {
                    surface.iter().any(|p| p[axis] >= lo[axis])
                        && surface.iter().any(|p| p[axis] <= hi[axis])
                })
            })
            .map(|[minimum, maximum]| Box3 {
                minimum: *minimum,
                maximum: *maximum,
            })
            .collect();
        // Local fluid cells replace water at its live elevation.
        for polygon in subtract_boxes(vec![surface], &boxes) {
            for edge in polygon[1..].windows(2) {
                for p in [polygon[0], edge[0], edge[1]] {
                    let point = glam::DVec3::from_array(p);
                    vertices.push(crate::ocean_geometry::OceanVertex {
                        position: (point - origin).as_vec3().to_array(),
                        normal: point.normalize().as_vec3().to_array(),
                    });
                }
            }
        }
    }
    vertices
}

#[derive(Clone, Copy)]
struct WaterSurfacePoint {
    // Keep the base on the conformed planar facet even at coarse/fine boundaries.
    base: glam::DVec3,
    scale: Option<f64>,
    depth_m: f64,
}

fn wet_surface(points: [WaterSurfacePoint; 3]) -> Vec<[f64; 3]> {
    let mut clipped = Vec::with_capacity(4);
    for i in 0..3 {
        let a = points[i];
        let b = points[(i + 1) % 3];
        let wet = a.depth_m > 0.0;
        if wet {
            clipped.push((a.base * a.scale.unwrap_or(1.0)).to_array());
        }
        if wet != (b.depth_m > 0.0) {
            let t = a.depth_m / (a.depth_m - b.depth_m);
            // Continue the wet endpoint's level across a catalog boundary.
            let start = a.base * a.scale.or(b.scale).unwrap_or(1.0);
            let end = b.base * b.scale.or(a.scale).unwrap_or(1.0);
            clipped.push(start.lerp(end, t).to_array());
        }
    }
    clipped
}
