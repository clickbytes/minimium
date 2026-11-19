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
    /// Exposed subsurface fraction and cached signed-field sky visibility.
    pub presentation: [f32; 2],
}

#[cfg(feature = "gpu")]
impl TerrainVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 10] = wgpu::vertex_attr_array![
        0 => Float32x3, 1 => Float32x3, 2 => Float32x3, 3 => Float32x3,
        4 => Float32x4, 5 => Float32x2, 6 => Float32x4, 7 => Float32x3,
        8 => Float32x3, 9 => Float32x2
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
    assert!(std::mem::size_of::<TerrainVertex>() == 120);
    assert!(std::mem::offset_of!(TerrainVertex, geometric_normal) == 88);
    assert!(std::mem::offset_of!(TerrainVertex, presentation) == 112);
};

pub fn vertices(
    geometry: &SectorGeometry,
    terrain: &PlanetTerrain,
    origin: [f32; 3],
) -> Result<Vec<TerrainVertex>, minimium_procgen::GenerationError> {
    let mut samples = std::collections::HashMap::new();
    let mut cuts = std::collections::HashMap::<[u64; 3], [f32; 2]>::new();
    let volume_base = terrain.volume_base();
    let volume_view = terrain
        .overlays()
        .volumes()
        .zip(volume_base.as_ref())
        .map(|(store, base)| store.view(base));
    let sample_excavation = |point, normal| {
        let view = volume_view.as_ref().ok_or_else(|| {
            minimium_procgen::GenerationError::materialization("missing excavation volume")
        })?;
        let depth = view.excavated_depth_m(point).ok_or_else(|| {
            minimium_procgen::GenerationError::materialization("missing excavation depth")
        })?;
        // Reach full substrate coverage after 10 cm of local removal.
        let cut = (depth / 0.10).clamp(0.0, 1.0) as f32;
        let sky = if cut > 0.0 {
            crate::terrain_occlusion::sky_visibility(
                |position| view.interpolated(position).map(|sample| sample.distance_m),
                point,
                normal,
            )
        } else {
            1.0
        };
        Ok::<_, minimium_procgen::GenerationError>([cut, sky])
    };
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
        for (index, point) in points.into_iter().enumerate() {
            let key = point.map(f64::to_bits);
            let sample = if let Some(sample) = samples.get(&key) {
                *sample
            } else {
                let sample = terrain.surface_sample(point)?;
                samples.insert(key, sample);
                sample
            };
            let override_surface = triangle.surface.map(|vertices| vertices[index]);
            let voxel_surface = override_surface.filter(|surface| {
                matches!(
                    surface.kind,
                    minimium_world::world::MapSurfaceKind::Excavation
                )
            });
            // Volume meshes also cover intact ground in each edited chunk. Their
            // payload describes subsurface soil/rock, not the original biome.
            let ground = override_surface.filter(|_| voxel_surface.is_none());
            let mut coverage = ground.map_or(sample.coverage, |surface| surface.coverage);
            let mut normal = ground.map_or(sample.normal, |surface| surface.normal);
            let mut presentation = [0.0, 1.0];
            if let Some(voxel) = voxel_surface {
                presentation = if let Some(cached) = cuts.get(&key) {
                    *cached
                } else {
                    let channels = sample_excavation(point, voxel.normal)?;
                    cuts.insert(key, channels);
                    channels
                };
                let cut = presentation[0];
                coverage =
                    std::array::from_fn(|i| coverage[i] * (1.0 - cut) + voxel.coverage[i] * cut);
                let voxel_normal = glam::Vec3::from_array(voxel.normal);
                normal = glam::Vec3::from_array(normal)
                    .lerp(voxel_normal, cut)
                    .try_normalize()
                    .unwrap_or(voxel_normal)
                    .to_array();
            }
            out.push(TerrainVertex {
                position: std::array::from_fn(|i| (point[i] - f64::from(origin[i])) as f32),
                // Curtains conceal cracks; lighting them as vertical cliffs
                // exposes a dark grid over otherwise flat water/snow.
                normal,
                planet_high: high.map(|v| v as f32),
                planet_low: std::array::from_fn(|i| (point[i] - high[i]) as f32),
                coverage_a: coverage[..4].try_into().unwrap(),
                coverage_b: coverage[4..].try_into().unwrap(),
                surface: ground.map_or(
                    [
                        sample.source_elevation_m,
                        sample.slope,
                        sample.water,
                        if voxel_surface.is_some() {
                            sample.elevation_m
                        } else {
                            sample.elevation_m / terrain.presentation_height_scale()
                        },
                    ],
                    |v| {
                        [
                            v.source_elevation_m,
                            v.slope,
                            v.water,
                            v.surface_elevation_m,
                        ]
                    },
                ),
                geometric_normal: face_normal,
                habitat: ground.map_or(
                    [
                        sample.landform.retention,
                        sample.landform.exposure,
                        f32::from(sample.mapped_coverage),
                    ],
                    |v| v.habitat,
                ),
                presentation,
            });
        }
    }
    Ok(out)
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
        let excavation = triangle.surface.is_some_and(|surface| {
            surface.iter().any(|vertex| {
                matches!(
                    vertex.kind,
                    minimium_world::world::MapSurfaceKind::Excavation
                )
            })
        });
        if excavation || !radial_curtain(points) {
            out.extend_from_slice(rendered);
        }
    }
    out
}
