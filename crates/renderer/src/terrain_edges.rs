//! Conform fine patch boundaries to their coarse neighbor, including cube seams.
//! Only presentation copies change; generator/collision geometry is immutable.
use crate::terrain_geometry::radial_curtain;
use minimium_procgen::{GenerationError, PlanetTerrain, SectorGeometry};
use minimium_world::world::{
    SectorKey, PLANET_DENSE_SECTORS_PER_FACE, PLANET_SAMPLE_COORDINATE_SCALE,
};
use std::{collections::HashMap, sync::Arc};

type Patch = (SectorKey, u8, Arc<SectorGeometry>);
type Line = (usize, [i32; 3]);
struct Entry {
    source: Arc<SectorGeometry>,
    steps: [i32; 4],
    ground: Arc<SectorGeometry>,
    ocean: Option<Arc<SectorGeometry>>,
}

#[derive(Clone, Copy)]
struct Edge {
    line: Line,
    from: i32,
    to: i32,
    step: i32,
}

#[derive(Default)]
pub struct EdgeConformance {
    entries: HashMap<(SectorKey, u8), Entry>,
}

impl EdgeConformance {
    pub fn ocean(&self, key: SectorKey, lod: u8) -> Option<&SectorGeometry> {
        self.entries.get(&(key, lod))?.ocean.as_deref()
    }

    pub fn prepare(
        &mut self,
        patches: Vec<Patch>,
        terrain: &PlanetTerrain,
    ) -> Result<Vec<Patch>, GenerationError> {
        let mut lines: HashMap<Line, Vec<Edge>> = HashMap::new();
        let edges: Vec<_> = patches
            .iter()
            .map(|(key, lod, _)| edges(*key, *lod))
            .collect();
        for edge in edges.iter().flatten().flatten() {
            lines.entry(edge.line).or_default().push(*edge);
        }
        let mut retained = HashMap::new();
        let result = patches
            .into_iter()
            .zip(edges)
            .map(|((key, lod, source), edges)| {
                let Some(edges) = edges else {
                    return Ok((key, lod, source));
                };
                let steps = edges.map(|edge| {
                    lines[&edge.line]
                        .iter()
                        .filter(|other| other.from <= edge.from && other.to >= edge.to)
                        .map(|other| other.step)
                        .max()
                        .unwrap_or(edge.step)
                });
                let cached = self
                    .entries
                    .remove(&(key, lod))
                    .filter(|entry| Arc::ptr_eq(&entry.source, &source) && entry.steps == steps);
                let entry = if let Some(entry) = cached {
                    entry
                } else {
                    let ground = conform(&source, edges, steps, terrain, false)?;
                    let (ground, ocean) = if terrain.is_earth() {
                        let sphere = crate::ocean_geometry::sphere_patch(&source, terrain);
                        let sea = conform(&sphere, edges, steps, terrain, true)?;
                        let (land, water) = crate::shoreline::partition(&ground, &sea);
                        (land, Some(Arc::new(water)))
                    } else {
                        (ground, None)
                    };
                    Entry {
                        ground: Arc::new(ground),
                        ocean,
                        source,
                        steps,
                    }
                };
                let ground = entry.ground.clone();
                retained.insert((key, lod), entry);
                Ok((key, lod, ground))
            })
            .collect::<Result<Vec<_>, GenerationError>>()?;
        self.entries = retained;
        Ok(result)
    }
}

pub(crate) fn preconform_bases(
    patches: Vec<Patch>,
    terrain: &PlanetTerrain,
) -> Result<Vec<Patch>, GenerationError> {
    let edge_sets: Vec<_> = patches
        .iter()
        .map(|(key, lod, _)| edges(*key, *lod))
        .collect();
    let mut lines: HashMap<Line, Vec<Edge>> = HashMap::new();
    for edge in edge_sets.iter().flatten().flatten() {
        lines.entry(edge.line).or_default().push(*edge);
    }
    patches
        .into_iter()
        .zip(edge_sets)
        .map(|((key, lod, source), edge_set)| {
            let Some(edge_set) = edge_set else {
                return Ok((key, lod, source));
            };
            let steps = edge_set.map(|edge| {
                lines[&edge.line]
                    .iter()
                    .filter(|other| other.from <= edge.from && other.to >= edge.to)
                    .map(|other| other.step)
                    .max()
                    .unwrap_or(edge.step)
            });
            let geometry = Arc::new(conform(&source, edge_set, steps, terrain, false)?);
            Ok((key, lod, geometry))
        })
        .collect()
}

fn edges(key: SectorKey, lod: u8) -> Option<[Edge; 4]> {
    let SectorKey::Planet(key) = key else {
        return None;
    };
    if !(16..=30).contains(&lod) {
        return None;
    }
    let span = 1_i32 << (lod - 16);
    let scale = PLANET_SAMPLE_COORDINATE_SCALE;
    let unit = 2 * scale / i32::from(PLANET_DENSE_SECTORS_PER_FACE);
    let (u, v) = key.coordinates();
    let u = -scale + i32::from(u) * unit;
    let v = -scale + i32::from(v) * unit;
    let width = span * unit;
    let point = |u, v| {
        key.face()
            .cube_sample(u, v)
            .expect("selected patch bounds")
            .coordinates()
    };
    let corners = [
        point(u, v),
        point(u + width, v),
        point(u + width, v + width),
        point(u, v + width),
    ];
    Some(std::array::from_fn(|i| {
        let a = corners[i];
        let b = corners[(i + 1) % 4];
        let axis = (0..3)
            .find(|&axis| a[axis] != b[axis])
            .expect("nonzero edge");
        let mut fixed = a;
        fixed[axis] = 0;
        Edge {
            line: (axis, fixed),
            from: a[axis].min(b[axis]),
            to: a[axis].max(b[axis]),
            step: width / minimium_procgen::PLANET_RENDER_BLOCK_SUBDIVISIONS as i32,
        }
    }))
}

fn conform(
    source: &SectorGeometry,
    edges: [Edge; 4],
    steps: [i32; 4],
    terrain: &PlanetTerrain,
    ocean: bool,
) -> Result<SectorGeometry, GenerationError> {
    let mut geometry = SectorGeometry {
        object_triangles: if ocean {
            Vec::new()
        } else {
            source.object_triangles.clone()
        },
        boxes: source.boxes.clone(),
        triangles: Vec::with_capacity(source.triangles.len()),
    };
    let tolerance = (PLANET_SAMPLE_COORDINATE_SCALE as f64 * 3.0 / terrain.radius_mm() as f64)
        .ceil()
        .max(1.0) as i32;
    let mut points = HashMap::new();
    let mut segments = HashMap::new();
    let protected: std::collections::HashSet<_> = source
        .triangles
        .iter()
        .filter_map(|triangle| Some((triangle.precise_vertices?, triangle.surface?)))
        .flat_map(|(points, surfaces)| {
            surfaces
                .into_iter()
                .zip(points)
                .filter_map(|(surface, point)| {
                    matches!(
                        surface.kind,
                        minimium_world::world::MapSurfaceKind::Excavation
                    )
                    .then_some(point)
                })
        })
        .collect();
    for triangle in &source.triangles {
        let Some(precise) = triangle.precise_vertices else {
            geometry.triangles.push(*triangle);
            continue;
        };
        let excavation = triangle.surface.is_some_and(|surface| {
            surface.iter().any(|vertex| {
                matches!(
                    vertex.kind,
                    minimium_world::world::MapSurfaceKind::Excavation
                )
            })
        });
        if !excavation && radial_curtain(precise.map(|p| p.map(|v| v as f64 / 1000.0))) {
            continue;
        }
        let mut next = *triangle;
        let precise = precise.map(|point| {
            points
                .entry(point)
                .or_insert_with(|| {
                    if protected.contains(&point) {
                        return Ok::<_, GenerationError>(point);
                    }
                    let cube = minimium_procgen::terrain_surface::key(point.map(|v| v as f64))
                        .coordinates();
                    for (edge, step) in edges.iter().zip(steps) {
                        let (axis, fixed) = edge.line;
                        if step <= edge.step
                            || (0..3).any(|a| a != axis && (cube[a] - fixed[a]).abs() > tolerance)
                        {
                            continue;
                        }
                        let scale = PLANET_SAMPLE_COORDINATE_SCALE;
                        // Both shells use the same canonical fraction. A projection
                        // of each displaced point would give ground and sea different
                        // fractions and could move their intersection at a LOD seam.
                        let value = (((cube[axis] + scale + edge.step / 2) / edge.step)
                            * edge.step
                            - scale)
                            .clamp(edge.from, edge.to);
                        let cell = ((value + scale) / step * step - scale).min(scale - step);
                        let (a, b) = segments
                            .entry((edge.line, cell, step))
                            .or_insert_with(|| {
                                let sample = |value| {
                                    let mut c = fixed;
                                    c[axis] = value;
                                    let key =
                                        minimium_procgen::terrain_surface::key(c.map(f64::from));
                                    let point = if ocean {
                                        terrain.sea_position(key)
                                    } else {
                                        terrain.ground_sample(key)?.position_mm
                                    };
                                    Ok::<_, GenerationError>(glam::DVec3::from_array(
                                        point.coordinates().map(|v| v as f64),
                                    ))
                                };
                                Ok::<_, GenerationError>((sample(cell)?, sample(cell + step)?))
                            })
                            .clone()?;
                        // Every resulting point lies on the exact coarse triangle edge.
                        let d = b - a;
                        let t = f64::from(value - cell) / f64::from(step);
                        return Ok((a + d * t).to_array().map(|v| v.round() as i64));
                    }
                    Ok(point)
                })
                .clone()
        });
        let [a, b, c] = precise;
        let precise = [a?, b?, c?];
        next.precise_vertices = Some(precise);
        next.vertices = precise.map(|p| p.map(|v| v as f32 / 1000.0));
        geometry.triangles.push(next);
    }
    Ok(geometry)
}
