use crate::terrain_color::{mostly_ocean, terrain_color};
use crate::{PlanetSurfaceSpec, PlanetTerrain, SectorGeometry};
use minimium_core::world::{
    MapTriangle, PlanetFace, PlanetSectorKey, WorldManifest, PLANET_DENSE_SECTORS_PER_FACE,
    PLANET_SECTORS_PER_FACE,
};

/// The collision lattice is fixed independently of the renderer's patch detail.
/// Neighboring sectors, including those on different cube faces, sample the
/// same canonical cube points at their shared border.
pub const PLANET_COLLISION_SUBDIVISIONS: u32 = 16;
pub const PLANET_RADIUS_MM: u64 = 10_000_000;
pub const PLANET_RELIEF_MM: u64 = 30_000;

pub fn planet_surface() -> PlanetSurfaceSpec {
    PlanetSurfaceSpec::new(PLANET_RADIUS_MM, PLANET_RELIEF_MM)
        .expect("the fixed planet surface is valid")
}

pub fn planet_surface_for(manifest: WorldManifest) -> PlanetSurfaceSpec {
    if let Some(definition) = manifest.planet() {
        PlanetSurfaceSpec::new(
            definition.radius_mm,
            u64::from(definition.mountain_limit_mm),
        )
        .expect("validated planet definition has a valid surface")
    } else {
        planet_surface()
    }
}

pub fn planet_sector_count(manifest: WorldManifest) -> u16 {
    if manifest.planet().is_some() {
        PLANET_DENSE_SECTORS_PER_FACE
    } else {
        PLANET_SECTORS_PER_FACE
    }
}

/// Resolve an arbitrary globe click through the same terrain definition as
/// streamed sectors. The caller decides whether ocean is a valid destination.
pub fn planet_point_at_direction(
    manifest: WorldManifest,
    direction: [f32; 3],
) -> Option<crate::PlanetPoint> {
    if manifest.topology() != minimium_core::world::WorldTopology::Planet
        || !direction.iter().all(|component| component.is_finite())
    {
        return None;
    }
    let [x, y, z] = direction.map(f64::from);
    let dominant = x.abs().max(y.abs()).max(z.abs());
    if dominant < 0.5 {
        return None;
    }
    let (face, u, v) = if x.abs() == dominant {
        (
            if x > 0.0 {
                PlanetFace::PositiveX
            } else {
                PlanetFace::NegativeX
            },
            y / dominant,
            z / dominant,
        )
    } else if y.abs() == dominant {
        (
            if y > 0.0 {
                PlanetFace::PositiveY
            } else {
                PlanetFace::NegativeY
            },
            x / dominant,
            z / dominant,
        )
    } else {
        (
            if z > 0.0 {
                PlanetFace::PositiveZ
            } else {
                PlanetFace::NegativeZ
            },
            x / dominant,
            y / dominant,
        )
    };
    let scale = f64::from(minimium_core::world::PLANET_SAMPLE_COORDINATE_SCALE);
    let sample = face.cube_sample((u * scale).round() as i32, (v * scale).round() as i32)?;
    let mut terrain = PlanetTerrain::new(manifest.generation(), planet_surface_for(manifest))?;
    if let Some(definition) = manifest.planet() {
        terrain = terrain.with_definition(definition);
    }
    Some(terrain.sample(sample))
}

pub fn generate_planet_sector(manifest: WorldManifest, key: PlanetSectorKey) -> SectorGeometry {
    let mut terrain = PlanetTerrain::new(manifest.generation(), planet_surface_for(manifest))
        .expect("the planet generation quantum fits the surface");
    if let Some(definition) = manifest.planet() {
        terrain = terrain.with_definition(definition);
    }
    let subdivisions = if manifest.planet().is_some() {
        32
    } else {
        PLANET_COLLISION_SUBDIVISIONS
    };
    let count = planet_sector_count(manifest);
    let edge = subdivisions as usize;
    let mut points = Vec::with_capacity((edge + 1) * (edge + 1));
    let mut ocean = Vec::with_capacity((edge + 1) * (edge + 1));
    for v in 0..=subdivisions {
        for u in 0..=subdivisions {
            let sample = key
                .cube_sample_with_count(u, v, subdivisions, count)
                .expect("sector lattice point is valid");
            let point = terrain.sample(sample);
            points.push(point.position_mm.coordinates());
            ocean.push(point.ocean);
        }
    }
    let mut triangles = Vec::with_capacity(edge * edge * 2);
    for v in 0..edge {
        for u in 0..edge {
            let a = points[v * (edge + 1) + u];
            let b = points[(v + 1) * (edge + 1) + u];
            let c = points[v * (edge + 1) + u + 1];
            let d = points[(v + 1) * (edge + 1) + u + 1];
            for (vertices, water) in [
                (
                    [a, b, c],
                    ocean[v * (edge + 1) + u]
                        || ocean[(v + 1) * (edge + 1) + u]
                        || ocean[v * (edge + 1) + u + 1],
                ),
                (
                    [c, b, d],
                    ocean[v * (edge + 1) + u + 1]
                        || ocean[(v + 1) * (edge + 1) + u]
                        || ocean[(v + 1) * (edge + 1) + u + 1],
                ),
            ] {
                add_precise_triangle(&mut triangles, vertices, water, true);
            }
        }
    }
    SectorGeometry {
        boxes: Vec::new(),
        triangles,
    }
}

/// Render-only fan mesh. Every outer edge uses the same 16 canonical samples
/// regardless of interior detail, so adjacent detail levels meet exactly.
pub fn generate_planet_render_patch(
    manifest: WorldManifest,
    key: PlanetSectorKey,
    subdivisions: u32,
) -> SectorGeometry {
    assert!(subdivisions.is_power_of_two() && (1..=16).contains(&subdivisions));
    let mut terrain = PlanetTerrain::new(manifest.generation(), planet_surface_for(manifest))
        .expect("the planet generation quantum fits the surface");
    if let Some(definition) = manifest.planet() {
        terrain = terrain.with_definition(definition);
    }
    let count = planet_sector_count(manifest);
    let mut triangles = Vec::new();
    let edge_step = 16 / subdivisions;
    let point = |u: u32, v: u32| {
        let sample = key
            .cube_sample_with_count(u, v, 16, count)
            .expect("render edge sample is valid");
        terrain.sample(sample)
    };
    for cell_v in 0..subdivisions {
        for cell_u in 0..subdivisions {
            let u0 = cell_u * edge_step;
            let v0 = cell_v * edge_step;
            let u1 = u0 + edge_step;
            let v1 = v0 + edge_step;
            let center_sample = key
                .cube_sample_with_count(cell_u * 2 + 1, cell_v * 2 + 1, subdivisions * 2, count)
                .expect("render cell center is valid");
            let center = terrain.sample(center_sample);
            let mut border = Vec::with_capacity(4 * edge_step as usize);
            let edges = [
                ((u0, v0), (u1, v0), v0 == 0),
                ((u1, v0), (u1, v1), u1 == 16),
                ((u1, v1), (u0, v1), v1 == 16),
                ((u0, v1), (u0, v0), u0 == 0),
            ];
            for ((from_u, from_v), (to_u, to_v), outer) in edges {
                let steps = if outer { edge_step } else { 1 };
                for index in 0..steps {
                    let fraction = index as f32 / steps as f32;
                    let u =
                        (from_u as f32 + (to_u as f32 - from_u as f32) * fraction).round() as u32;
                    let v =
                        (from_v as f32 + (to_v as f32 - from_v as f32) * fraction).round() as u32;
                    border.push(point(u, v));
                }
            }
            for index in 0..border.len() {
                let next = border[(index + 1) % border.len()];
                let current = border[index];
                add_precise_triangle(
                    &mut triangles,
                    [
                        center.position_mm.coordinates(),
                        current.position_mm.coordinates(),
                        next.position_mm.coordinates(),
                    ],
                    center.ocean || current.ocean || next.ocean,
                    false,
                );
                let water = mostly_ocean([center.ocean, current.ocean, next.ocean]);
                let height = [center, current, next]
                    .map(|point| {
                        point.height_mm as i64 - terrain.radius_mm() as i64 - terrain.sea_level_mm()
                    })
                    .into_iter()
                    .sum::<i64>()
                    / 3;
                triangles.last_mut().expect("triangle was added").color =
                    terrain_color(height, water);
            }
        }
    }
    SectorGeometry {
        boxes: Vec::new(),
        triangles,
    }
}

/// A coarse quadtree leaf spanning `span` fine sectors. Edge skirts conceal
/// the different sampling density at neighboring quadtree levels.
pub fn generate_planet_render_block(
    manifest: WorldManifest,
    face: PlanetFace,
    u: u16,
    v: u16,
    span: u16,
) -> SectorGeometry {
    const STEPS: usize = 16;
    let count = u32::from(planet_sector_count(manifest));
    let scale = minimium_core::world::PLANET_SAMPLE_COORDINATE_SCALE;
    let mut terrain = PlanetTerrain::new(manifest.generation(), planet_surface_for(manifest))
        .expect("the planet generation quantum fits the surface");
    if let Some(definition) = manifest.planet() {
        terrain = terrain.with_definition(definition);
    }
    let mut points = Vec::with_capacity((STEPS + 1) * (STEPS + 1));
    let mut ocean = Vec::with_capacity((STEPS + 1) * (STEPS + 1));
    let mut heights = Vec::with_capacity((STEPS + 1) * (STEPS + 1));
    for row in 0..=STEPS {
        for column in 0..=STEPS {
            let grid_u = u32::from(u) * STEPS as u32 + u32::from(span) * column as u32;
            let grid_v = u32::from(v) * STEPS as u32 + u32::from(span) * row as u32;
            let cube_u = -scale
                + (2_i64 * i64::from(scale) * i64::from(grid_u) / (i64::from(count) * STEPS as i64))
                    as i32;
            let cube_v = -scale
                + (2_i64 * i64::from(scale) * i64::from(grid_v) / (i64::from(count) * STEPS as i64))
                    as i32;
            let sample = face
                .cube_sample(cube_u, cube_v)
                .expect("block lies on face");
            let point = terrain.sample(sample);
            points.push(point.position_mm.coordinates());
            ocean.push(point.ocean);
            heights
                .push(point.height_mm as i64 - terrain.radius_mm() as i64 - terrain.sea_level_mm());
        }
    }
    let mut triangles = Vec::with_capacity(STEPS * STEPS * 2 + STEPS * 8);
    let mut add = |vertices: [[i64; 3]; 3], color: [f32; 3]| {
        add_precise_triangle(&mut triangles, vertices, false, false);
        triangles.last_mut().expect("triangle was added").color = color;
    };
    for row in 0..STEPS {
        for column in 0..STEPS {
            let a = points[row * (STEPS + 1) + column];
            let b = points[(row + 1) * (STEPS + 1) + column];
            let c = points[row * (STEPS + 1) + column + 1];
            let d = points[(row + 1) * (STEPS + 1) + column + 1];
            let ia = row * (STEPS + 1) + column;
            let ib = (row + 1) * (STEPS + 1) + column;
            let ic = ia + 1;
            let id = ib + 1;
            for (vertices, indices) in [([a, b, c], [ia, ib, ic]), ([c, b, d], [ic, ib, id])] {
                let water = mostly_ocean(indices.map(|index| ocean[index]));
                let height = indices.map(|index| heights[index]).into_iter().sum::<i64>() / 3;
                add(vertices, terrain_color(height, water));
            }
        }
    }
    let skirt_depth = manifest
        .planet()
        .map(|definition| definition.mountain_limit_mm as f32 / 1000.0 + 10.0)
        .unwrap_or(5.0);
    let lowered = |point: [i64; 3]| {
        let length = point
            .iter()
            .map(|component| (*component as f64).powi(2))
            .sum::<f64>()
            .sqrt();
        point.map(|component| {
            (component as f64 * (1.0 - f64::from(skirt_depth) * 1000.0 / length)).round() as i64
        })
    };
    for edge in 0..4 {
        for index in 0..STEPS {
            let (ia, ib) = match edge {
                0 => (index, index + 1),
                1 => (
                    index * (STEPS + 1) + STEPS,
                    (index + 1) * (STEPS + 1) + STEPS,
                ),
                2 => (STEPS * (STEPS + 1) + index, STEPS * (STEPS + 1) + index + 1),
                _ => (index * (STEPS + 1), (index + 1) * (STEPS + 1)),
            };
            let (a, b) = (points[ia], points[ib]);
            let color = terrain_color((heights[ia] + heights[ib]) / 2, ocean[ia] && ocean[ib]);
            add([a, lowered(a), b], color);
            add([b, lowered(a), lowered(b)], color);
        }
    }
    SectorGeometry {
        boxes: Vec::new(),
        triangles,
    }
}

fn add_precise_triangle(
    triangles: &mut Vec<MapTriangle>,
    mut vertices: [[i64; 3]; 3],
    water: bool,
    solid: bool,
) {
    let a = vertices[0].map(|x| x as f64);
    let b = vertices[1].map(|x| x as f64);
    let c = vertices[2].map(|x| x as f64);
    let ab: [f64; 3] = std::array::from_fn(|axis| b[axis] - a[axis]);
    let ac: [f64; 3] = std::array::from_fn(|axis| c[axis] - a[axis]);
    let normal = [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ];
    if normal.iter().zip(a).map(|(n, p)| n * p).sum::<f64>() < 0.0 {
        vertices.swap(1, 2);
    }
    triangles.push(MapTriangle {
        vertices: vertices.map(|point| point.map(|x| x as f32 / 1000.0)),
        precise_vertices: Some(vertices),
        color: if water {
            [0.08, 0.24, 0.48]
        } else {
            [0.18, 0.42, 0.19]
        },
        solid,
    });
}
