//! A separate sea-level sphere using the ground selection's canonical lattice.
//! Matching tessellation avoids a coarse spherical ocean swallowing coastal land
//! through the sagitta of differently sized triangle chords.
use minimium_procgen::{PlanetTerrain, SectorGeometry};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct OceanVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

impl OceanVertex {
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
            wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRIBUTES,
        }
    }
}

pub(crate) fn sphere_patch(source: &SectorGeometry, terrain: &PlanetTerrain) -> SectorGeometry {
    let mut points = std::collections::HashMap::new();
    let triangles = source
        .triangles
        .iter()
        .filter_map(|triangle| {
            let precise = triangle.precise_vertices?;
            if crate::terrain_geometry::radial_curtain(
                precise.map(|p| p.map(|v| v as f64 / 1000.0)),
            ) {
                return None;
            }
            let precise = precise.map(|p| {
                *points.entry(p).or_insert_with(|| {
                    let key = minimium_procgen::terrain_surface::key(p.map(|v| v as f64));
                    terrain.sea_position(key).coordinates()
                })
            });
            Some(minimium_world::world::MapTriangle {
                vertices: precise.map(|p| p.map(|v| v as f32 / 1000.0)),
                precise_vertices: Some(precise),
                color: [0.012, 0.042, 0.068],
                solid: false,
            })
        })
        .collect();
    SectorGeometry {
        boxes: Vec::new(),
        triangles,
    }
}

pub fn vertices(geometry: &SectorGeometry, origin: [f32; 3]) -> Vec<OceanVertex> {
    let origin = glam::DVec3::from_array(origin.map(f64::from));
    geometry
        .triangles
        .iter()
        .flat_map(|triangle| {
            let positions = triangle.precise_vertices.map_or_else(
                || triangle.vertices.map(|p| p.map(f64::from)),
                |p| p.map(|p| p.map(|v| v as f64 / 1000.0)),
            );
            positions.map(|p| {
                let p = glam::DVec3::from_array(p);
                OceanVertex {
                    position: (p - origin).as_vec3().to_array(),
                    normal: p.normalize().as_vec3().to_array(),
                }
            })
        })
        .collect()
}

/// Remove triangles wholly behind their matching sea facet before material and
/// shadow preparation. Coastal triangles remain intact; GPU depth finds the shore.
pub(crate) fn exposed_ground(ground: &SectorGeometry, sea: &SectorGeometry) -> SectorGeometry {
    if ground.triangles.len() != sea.triangles.len() {
        return ground.clone();
    }
    let triangles = ground
        .triangles
        .iter()
        .zip(&sea.triangles)
        .filter_map(|(g, s)| {
            let (Some(gp), Some(sp)) = (g.precise_vertices, s.precise_vertices) else {
                return Some(*g);
            };
            let sp = sp.map(|p| glam::DVec3::from_array(p.map(|v| v as f64)));
            let n = (sp[1] - sp[0]).cross(sp[2] - sp[0]).normalize_or_zero();
            // A two-millimetre margin retains uncertain, nearly coplanar land.
            let exposed = gp
                .iter()
                .any(|p| (glam::DVec3::from_array(p.map(|v| v as f64)) - sp[0]).dot(n) >= -2.0);
            exposed.then_some(*g)
        })
        .collect();
    SectorGeometry {
        boxes: ground.boxes.clone(),
        triangles,
    }
}

const _: () = assert!(std::mem::size_of::<OceanVertex>() == 24);
