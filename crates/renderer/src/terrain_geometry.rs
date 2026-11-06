//! Typed surface presentation data, separate from authored objects and collision.
use minimium_procgen::{PlanetTerrain, SectorGeometry};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TerrainVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub planet_high: [f32; 3],
    pub planet_low: [f32; 3],
    pub coverage_a: [f32; 4],
    pub coverage_b: [f32; 2],
    /// Source height, slope, ocean presence, exaggerated height in source metres.
    pub surface: [f32; 4],
    pub geometric_normal: [f32; 3],
    pub habitat: [f32; 3],
}

impl TerrainVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 9] = wgpu::vertex_attr_array![
        0 => Float32x3, 1 => Float32x3, 2 => Float32x3, 3 => Float32x3,
        4 => Float32x4, 5 => Float32x2, 6 => Float32x4, 7 => Float32x3, 8 => Float32x3
    ];
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

const _: () = {
    assert!(std::mem::size_of::<TerrainVertex>() == 112);
    assert!(std::mem::offset_of!(TerrainVertex, geometric_normal) == 88);
};

pub fn vertices(
    geometry: &SectorGeometry,
    terrain: &PlanetTerrain,
    origin: [f32; 3],
) -> Vec<TerrainVertex> {
    let mut samples = std::collections::HashMap::new();
    let mut out = Vec::with_capacity(geometry.triangles.len() * 3);
    for triangle in &geometry.triangles {
        let points = triangle
            .precise_vertices
            .map(|points| points.map(|p| p.map(|v| v as f64 / 1000.0)))
            .unwrap_or_else(|| triangle.vertices.map(|p| p.map(f64::from)));
        // One anchor per primitive keeps the large component constant during interpolation.
        let high = points[0].map(|v| (v / 1024.0).floor() * 1024.0);
        // Match the actual f32 positions used in both raster passes, including
        // their origin-relative rounding; the sampled smooth normal is separate.
        let rendered = points.map(|p| {
            glam::DVec3::from_array(std::array::from_fn(|i| {
                f64::from((p[i] - f64::from(origin[i])) as f32)
            }))
        });
        let face_normal = (rendered[1] - rendered[0])
            .cross(rendered[2] - rendered[0])
            .normalize_or_zero()
            .as_vec3()
            .to_array();
        for point in points {
            let key = point.map(f64::to_bits);
            let sample = *samples
                .entry(key)
                .or_insert_with(|| terrain.surface_sample(point));
            out.push(TerrainVertex {
                position: std::array::from_fn(|i| (point[i] - f64::from(origin[i])) as f32),
                // Curtains conceal cracks; lighting them as vertical cliffs
                // exposes a dark grid over otherwise flat water/snow.
                normal: sample.normal,
                planet_high: high.map(|v| v as f32),
                planet_low: std::array::from_fn(|i| (point[i] - high[i]) as f32),
                coverage_a: sample.coverage[..4].try_into().unwrap(),
                coverage_b: sample.coverage[4..].try_into().unwrap(),
                surface: [
                    sample.source_elevation_m,
                    sample.slope,
                    f32::from(terrain.is_earth()),
                    sample.elevation_m / terrain.presentation_height_scale(),
                ],
                geometric_normal: face_normal,
                habitat: [
                    sample.landform.retention,
                    sample.landform.exposure,
                    f32::from(sample.mapped_coverage),
                ],
            });
        }
    }
    out
}

/// LOD curtains have a radial edge: two positions on the same planet ray.
/// Detect positions, never RGB or `solid` (render surfaces are also non-solid).
pub(crate) fn radial_curtain(points: [[f64; 3]; 3]) -> bool {
    (0..3).any(|i| {
        let a = glam::DVec3::from_array(points[i]);
        let b = glam::DVec3::from_array(points[(i + 1) % 3]);
        let shortest = a.length().min(b.length());
        shortest > 0.0
            && (a.length() - b.length()).abs() > 0.01
            && a.normalize().cross(b.normalize()).length() * shortest < 0.003
    })
}

/// Retain visible crack concealment, but do not cast artificial skirt shadows.
pub fn shadow_vertices(
    geometry: &SectorGeometry,
    vertices: Vec<crate::geometry::Vertex>,
) -> Vec<crate::geometry::Vertex> {
    let objects = geometry.boxes.len() * 36;
    let mut out = Vec::with_capacity(vertices.len());
    out.extend_from_slice(&vertices[..objects]);
    for (triangle, rendered) in geometry
        .triangles
        .iter()
        .zip(vertices[objects..].chunks_exact(3))
    {
        let points = triangle
            .precise_vertices
            .map(|p| p.map(|v| v.map(|c| c as f64 / 1000.0)))
            .unwrap_or_else(|| triangle.vertices.map(|p| p.map(f64::from)));
        if !radial_curtain(points) {
            out.extend_from_slice(rendered);
        }
    }
    out
}
