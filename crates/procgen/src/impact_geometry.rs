//! The same bounded rock mesh is used by rendering and collision.
use crate::PlanetTerrain;
use minimium_world::{terrain_edits::project, world::MapTriangle};

pub(crate) fn remnants(terrain: &PlanetTerrain, triangles: &[MapTriangle]) -> Vec<MapTriangle> {
    let mut objects = Vec::new();
    let Some(edits) = terrain.inputs().remnants.as_deref() else {
        return objects;
    };
    if triangles.is_empty() {
        return objects;
    }
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for triangle in triangles.iter() {
        for (index, p) in triangle.vertices.iter().enumerate() {
            let p = triangle
                .precise_vertices
                .map_or(p.map(f64::from), |v| v[index].map(|x| x as f64 / 1000.0));
            for i in 0..3 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
        }
    }
    let patch_center = std::array::from_fn(|i| (lo[i] + hi[i]) * 0.5);
    let patch_bound =
        minimium_world::terrain_edits::length(std::array::from_fn(|i| hi[i] - lo[i])) * 0.5;
    for body in edits.nearby(patch_center, patch_bound + 10.0) {
        let radius = body.radius_m().clamp(0.45, 3.5);
        // Broad phase before terrain sampling: most saved remnants are far away.
        let reach = body.crater_radius_m * 0.4 + radius * 2.0;
        if (0..3).any(|i| body.position[i] + reach < lo[i] || body.position[i] - reach > hi[i]) {
            continue;
        }
        let up = project(body.position, 1.0);
        let height = terrain
            .ground_sample(crate::terrain_surface::key(up))
            .height_mm as f64
            / 1000.0;
        let center = project(up, height + radius * 0.55);
        if (0..3).any(|i| center[i] + radius < lo[i] || center[i] - radius > hi[i] + radius * 2.0) {
            continue;
        }
        let mut vertices = [[0.0; 3]; 8];
        for (i, vertex) in vertices.iter_mut().enumerate() {
            let corner = [
                if i & 1 == 0 { -1.0 } else { 1.0 },
                if i & 2 == 0 { -1.0 } else { 1.0 },
                if i & 4 == 0 { -1.0 } else { 1.0 },
            ];
            let jitter = 0.82 + ((body.seed.rotate_left(i as u32 * 7) % 100) as f64) * 0.003;
            let shape = project(corner, radius * jitter);
            *vertex = std::array::from_fn(|axis| center[axis] + shape[axis]);
        }
        for indices in [
            [0, 2, 3],
            [0, 3, 1],
            [4, 5, 7],
            [4, 7, 6],
            [0, 1, 5],
            [0, 5, 4],
            [2, 6, 7],
            [2, 7, 3],
            [0, 4, 6],
            [0, 6, 2],
            [1, 3, 7],
            [1, 7, 5],
        ] {
            let points = indices.map(|i| vertices[i]);
            let tone = 0.11 + (body.seed % 13) as f32 * 0.004;
            objects.push(MapTriangle {
                vertices: points.map(|p| p.map(|x| x as f32)),
                precise_vertices: Some(points.map(|p| p.map(|x| (x * 1000.0).round() as i64))),
                color: [tone, tone * 0.91, tone * 0.82],
                solid: true,
                surface: None,
            });
        }
    }
    objects
}
