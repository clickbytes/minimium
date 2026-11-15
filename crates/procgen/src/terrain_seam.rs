//! Chunk-domain replacement with checked closed contours and a perimeter zipper.
use crate::{terrain_clip, terrain_contours, terrain_zipper, SectorGeometry};
use minimium_world::{
    terrain::{TerrainChunkKey, TERRAIN_CHUNK_M},
    world::{MapSurfaceVertex, MapTriangle},
};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExcavationGeometryError {
    Generation(crate::GenerationError),
    InvalidChunk,
    MissingGuardband,
    UnavailableSurface,
    OpenContour,
    OpenOldContour,
    OpenVoxelContour,
    UnmatchedContours,
    DegenerateTransition,
    InvalidTransitionContour,
    TransitionSelfIntersection,
    TransitionStripIntersection,
}

impl From<crate::GenerationError> for ExcavationGeometryError {
    fn from(error: crate::GenerationError) -> Self {
        Self::Generation(error)
    }
}

/// Replaces the old surface inside a guarded voxel-chunk domain. The callbacks
/// sample the same committed fields used by collision and Marching Cubes.
pub fn replace_chunk_domain(
    old: &SectorGeometry,
    contour_source: &[MapTriangle],
    voxel: &[MapTriangle],
    keys: &[TerrainChunkKey],
    mut old_surface: impl FnMut([i64; 3]) -> Option<MapSurfaceVertex>,
    mut voxel_surface: impl FnMut([i64; 3]) -> Option<MapSurfaceVertex>,
) -> Result<SectorGeometry, ExcavationGeometryError> {
    let active: BTreeSet<_> = keys.iter().copied().collect();
    if active.is_empty() || active.len() != keys.len() {
        return Err(ExcavationGeometryError::InvalidChunk);
    }
    let mut old_segments = Vec::new();
    let mut voxel_segments = Vec::new();
    let mut boxes = Vec::with_capacity(keys.len());
    for key in keys {
        if !key.valid() {
            return Err(ExcavationGeometryError::InvalidChunk);
        }
        let minimum = key.origin_m().map(|v| v * 1000.0);
        let maximum = minimum.map(|v| v + TERRAIN_CHUNK_M * 1000.0);
        boxes.push(terrain_clip::Box3 { minimum, maximum });
        for axis in 0..3 {
            for positive in [false, true] {
                let mut adjacent = key.coordinates;
                adjacent[axis] += if positive { 1 } else { -1 };
                if active.contains(&TerrainChunkKey {
                    world: key.world,
                    coordinates: adjacent,
                }) {
                    continue;
                }
                let plane = if positive {
                    maximum[axis]
                } else {
                    minimum[axis]
                };
                collect_contours(
                    contour_source,
                    minimum,
                    maximum,
                    axis,
                    plane,
                    false,
                    &mut old_segments,
                )?;
                collect_contours(
                    voxel,
                    minimum,
                    maximum,
                    axis,
                    plane,
                    true,
                    &mut voxel_segments,
                )?;
            }
        }
    }
    let old_loops = terrain_contours::closed_loops(&old_segments)
        .map_err(|_| ExcavationGeometryError::OpenOldContour)?;
    let voxel_loops = terrain_contours::closed_loops(&voxel_segments)
        .map_err(|_| ExcavationGeometryError::OpenVoxelContour)?;
    if old_loops.len() != voxel_loops.len() {
        return Err(ExcavationGeometryError::UnmatchedContours);
    }
    let clipped = clip_old(old, &boxes, &mut old_surface)?;
    let mut result = SectorGeometry {
        boxes: old.boxes.clone(),
        object_triangles: old.object_triangles.clone(),
        triangles: clipped,
    };
    result.triangles.extend_from_slice(voxel);
    let paired = pair_loops(&old_loops, &voxel_loops)?;
    let strips: Vec<_> = paired
        .iter()
        .map(|(old_loop, voxel_loop)| terrain_zipper::zipper(old_loop, voxel_loop))
        .collect::<Result<_, _>>()?;
    terrain_contours::validate_strip_intersections(&strips)?;
    for strip in strips {
        for (mut points, old_flags) in strip {
            let mut surfaces = std::array::from_fn(|i| {
                if old_flags[i] {
                    old_surface(points[i])
                } else {
                    voxel_surface(points[i])
                }
            });
            let [Some(a), Some(b), Some(c)] = surfaces else {
                return Err(ExcavationGeometryError::UnavailableSurface);
            };
            let mean = std::array::from_fn::<_, 3, _>(|i| {
                (a.normal[i] + b.normal[i] + c.normal[i]) as f64
            });
            if dot(cross(points[1], points[0], points[2]), mean) < 0.0 {
                points.swap(1, 2);
                surfaces.swap(1, 2);
            }
            let [a, b, c] = surfaces.map(Option::unwrap);
            result.triangles.push(MapTriangle {
                vertices: points.map(|p| p.map(|v| v as f32 / 1000.0)),
                precise_vertices: Some(points),
                color: [0.28, 0.22, 0.16],
                solid: true,
                surface: Some([a, b, c]),
            });
        }
    }
    Ok(result)
}

fn pair_loops<'a>(
    old: &'a [terrain_contours::Loop],
    voxel: &'a [terrain_contours::Loop],
) -> Result<Vec<(&'a terrain_contours::Loop, &'a terrain_contours::Loop)>, ExcavationGeometryError>
{
    if old.len() != voxel.len() {
        return Err(ExcavationGeometryError::UnmatchedContours);
    }
    let mut remaining: Vec<_> = voxel.iter().collect();
    let mut pairs = Vec::with_capacity(old.len());
    for source in old {
        let center = centroid(source);
        let Some((index, target)) = remaining.iter().enumerate().min_by(|(_, a), (_, b)| {
            distance2(center, centroid(a)).total_cmp(&distance2(center, centroid(b)))
        }) else {
            return Err(ExcavationGeometryError::UnmatchedContours);
        };
        pairs.push((source, *target));
        remaining.remove(index);
    }
    Ok(pairs)
}

fn centroid(points: &[terrain_contours::Point]) -> [f64; 3] {
    std::array::from_fn(|axis| {
        points.iter().map(|point| point[axis] as f64).sum::<f64>() / points.len() as f64
    })
}

fn distance2(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum()
}

fn collect_contours(
    triangles: &[MapTriangle],
    minimum: [f64; 3],
    maximum: [f64; 3],
    axis: usize,
    plane: f64,
    excavation: bool,
    out: &mut Vec<([i64; 3], [i64; 3])>,
) -> Result<(), ExcavationGeometryError> {
    let mut coplanar_edges = std::collections::BTreeMap::<([i64; 3], [i64; 3]), usize>::new();
    for triangle in triangles {
        let is_excavation = triangle.surface.is_some_and(|vertices| {
            vertices
                .iter()
                .all(|v| matches!(v.kind, minimium_world::world::MapSurfaceKind::Excavation))
        });
        if is_excavation != excavation || triangle.surface.is_none() {
            continue;
        }
        let Some(vertices) = triangle.precise_vertices else {
            if triangle.surface.is_some() {
                return Err(ExcavationGeometryError::UnavailableSurface);
            }
            continue;
        };
        let points = vertices.map(|p| p.map(|v| v as f64));
        if points.iter().all(|p| (p[axis] - plane).abs() <= 0.01) {
            let to_mm = |p: [f64; 3]| p.map(|v| v.round() as i64);
            for edge in 0..3 {
                let a = to_mm(points[edge]);
                let b = to_mm(points[(edge + 1) % 3]);
                let key = if a < b { (a, b) } else { (b, a) };
                *coplanar_edges.entry(key).or_default() += 1;
            }
            continue;
        }
        let mut hits = Vec::new();
        for edge in 0..3 {
            let a = points[edge];
            let b = points[(edge + 1) % 3];
            let da = a[axis] - plane;
            let db = b[axis] - plane;
            if da == 0.0 {
                hits.push(a);
            }
            if (da < 0.0 && db > 0.0) || (da > 0.0 && db < 0.0) {
                let t = da / (da - db);
                hits.push(std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t));
            }
        }
        hits.dedup_by(|a, b| (0..3).all(|i| (a[i] - b[i]).abs() < 0.01));
        if hits.len() > 2 {
            return Err(ExcavationGeometryError::DegenerateTransition);
        }
        if hits.len() != 2 {
            continue;
        }
        let Some((a, b)) = clip_segment_to_face(hits[0], hits[1], minimum, maximum, axis, plane)
        else {
            continue;
        };
        let to_mm = |p: [f64; 3]| p.map(|v| v.round() as i64);
        let (a, b) = (to_mm(a), to_mm(b));
        if a != b {
            out.push((a, b));
        }
    }
    out.extend(
        coplanar_edges
            .into_iter()
            .filter_map(|(edge, count)| (count % 2 == 1).then_some(edge)),
    );
    Ok(())
}

fn clip_segment_to_face(
    a: [f64; 3],
    b: [f64; 3],
    minimum: [f64; 3],
    maximum: [f64; 3],
    face_axis: usize,
    plane: f64,
) -> Option<([f64; 3], [f64; 3])> {
    let mut lo: f64 = 0.0;
    let mut hi: f64 = 1.0;
    for axis in 0..3 {
        if axis == face_axis {
            continue;
        }
        let delta = b[axis] - a[axis];
        if delta.abs() < 1.0e-12 {
            if a[axis] < minimum[axis] - 0.01 || a[axis] > maximum[axis] + 0.01 {
                return None;
            }
            continue;
        }
        let first = (minimum[axis] - a[axis]) / delta;
        let second = (maximum[axis] - a[axis]) / delta;
        lo = lo.max(first.min(second));
        hi = hi.min(first.max(second));
        if hi <= lo {
            return None;
        }
    }
    let point = |t: f64| {
        let mut point = std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t);
        point[face_axis] = plane;
        point
    };
    Some((point(lo), point(hi)))
}

fn clip_old(
    old: &SectorGeometry,
    boxes: &[terrain_clip::Box3],
    surface_at: &mut impl FnMut([i64; 3]) -> Option<MapSurfaceVertex>,
) -> Result<Vec<MapTriangle>, ExcavationGeometryError> {
    let mut output = Vec::new();
    for triangle in &old.triangles {
        if triangle.surface.is_none() {
            output.push(*triangle);
            continue;
        }
        let points = triangle
            .precise_vertices
            .map(|vertices| vertices.map(|p| p.map(|v| v as f64)))
            .unwrap_or_else(|| triangle.vertices.map(|p| p.map(|v| f64::from(v) * 1000.0)));
        let clipped = terrain_clip::subtract_boxes(vec![points.to_vec()], boxes);
        for polygon in clipped {
            for index in 1..polygon.len().saturating_sub(1) {
                let mm = [polygon[0], polygon[index], polygon[index + 1]]
                    .map(|p| p.map(|v| v.round() as i64));
                if area_squared(mm) < 1.0 {
                    continue;
                }
                let surface = [surface_at(mm[0]), surface_at(mm[1]), surface_at(mm[2])];
                let surface = if surface.iter().all(Option::is_some) {
                    Some(surface.map(Option::unwrap))
                } else {
                    return Err(ExcavationGeometryError::UnavailableSurface);
                };
                output.push(MapTriangle {
                    vertices: mm.map(|p| p.map(|v| v as f32 / 1000.0)),
                    precise_vertices: Some(mm),
                    color: triangle.color,
                    solid: triangle.solid,
                    surface,
                });
            }
        }
    }
    Ok(output)
}

fn area_squared(points: [[i64; 3]; 3]) -> f64 {
    cross(points[1], points[0], points[2])
        .iter()
        .map(|v| v * v)
        .sum()
}

fn cross(a: [i64; 3], b: [i64; 3], c: [i64; 3]) -> [f64; 3] {
    let ab = std::array::from_fn::<_, 3, _>(|i| (b[i] - a[i]) as f64);
    let ac = std::array::from_fn::<_, 3, _>(|i| (c[i] - a[i]) as f64);
    [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}
