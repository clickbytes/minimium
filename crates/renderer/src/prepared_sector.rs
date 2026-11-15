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
    ) -> Self {
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
    ) -> Self {
        // EdgeConformance already partitions land and sea at shared shore edges.
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
        let surface = terrain.map_or_else(Vec::new, |t| {
            crate::terrain_geometry::vertices(visible, t, origin)
        });
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
        Self {
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
        }
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
    use minimium_procgen::terrain_clip::clip_triangle_outside_boxes;
    let boxes = water.map_or_else(Vec::new, |(_, _, boxes)| boxes.to_vec());
    let origin = glam::DVec3::from_array(origin.map(f64::from));
    let mut vertices = Vec::new();
    for triangle in &geometry.triangles {
        let points = triangle.precise_vertices.map_or_else(
            || triangle.vertices.map(|point| point.map(f64::from)),
            |points| points.map(|point| point.map(|value| value as f64 / 1000.0)),
        );
        for polygon in clip_triangle_outside_boxes(points, &boxes) {
            let surfaces = water.map_or_else(
                || vec![polygon.clone()],
                |(catalog, snapshot, _)| water_surface_polygons(&polygon, catalog, snapshot),
            );
            for surface in surfaces {
                for index in 1..surface.len().saturating_sub(1) {
                    for point in [surface[0], surface[index], surface[index + 1]] {
                        let point = glam::DVec3::from_array(point);
                        vertices.push(crate::ocean_geometry::OceanVertex {
                            position: (point - origin).as_vec3().to_array(),
                            normal: point.normalize().as_vec3().to_array(),
                        });
                    }
                }
            }
        }
    }
    vertices
}

#[derive(Clone, Copy)]
struct WaterSurfacePoint {
    direction: [f64; 3],
    level_m: f64,
    depth_m: f64,
}

fn water_surface_polygons(
    polygon: &[[f64; 3]],
    catalog: &minimium_procgen::WaterCatalog,
    snapshot: &minimium_fluids::FluidSnapshot,
) -> Vec<Vec<[f64; 3]>> {
    let points: Vec<_> = polygon
        .iter()
        .map(|point| {
            let radius = point.iter().map(|value| value * value).sum::<f64>().sqrt();
            if radius <= f64::EPSILON {
                return WaterSurfacePoint {
                    direction: [0.0; 3],
                    level_m: 0.0,
                    depth_m: -1.0,
                };
            }
            let direction = point.map(|value| value / radius);
            let body_id = catalog.body_at_direction(direction);
            let reservoir = body_id.and_then(|id| snapshot.reservoirs().get(&id));
            let ground = catalog.ground_radius_at_direction_m(direction);
            let level = reservoir.and_then(|reservoir| reservoir.level_m().ok());
            match (level, ground) {
                (Some(level), Some(ground)) => WaterSurfacePoint {
                    direction,
                    level_m: level,
                    depth_m: level - ground,
                },
                _ => WaterSurfacePoint {
                    direction,
                    level_m: radius,
                    depth_m: -1.0,
                },
            }
        })
        .collect();
    let mut clipped = Vec::new();
    let mut output = Vec::new();
    for index in 0..points.len() {
        let a = points[index];
        let b = points[(index + 1) % points.len()];
        let a_inside = a.depth_m >= 0.0;
        let b_inside = b.depth_m >= 0.0;
        if a_inside {
            clipped.push(surface_position(a));
        }
        if a_inside != b_inside {
            let fraction = (a.depth_m / (a.depth_m - b.depth_m)).clamp(0.0, 1.0);
            let direction: [f64; 3] = std::array::from_fn(|axis| {
                a.direction[axis] + (b.direction[axis] - a.direction[axis]) * fraction
            });
            let length = direction
                .iter()
                .map(|value| value * value)
                .sum::<f64>()
                .sqrt();
            let direction = direction.map(|value| value / length.max(f64::EPSILON));
            let level_m = a.level_m + (b.level_m - a.level_m) * fraction;
            clipped.push(std::array::from_fn(|axis| direction[axis] * level_m));
        }
    }
    if clipped.len() >= 3 {
        output.push(clipped);
    }
    output
}

fn surface_position(point: WaterSurfacePoint) -> [f64; 3] {
    point.direction.map(|value| value * point.level_m)
}
