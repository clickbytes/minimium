//! Exterior edges of the exact quantized mesh that will be published.
use crate::{terrain_contours::Point, ExcavationGeometryError};
use minimium_world::{
    terrain::{TerrainChunkKey, TERRAIN_CHUNK_M},
    world::{MapSurfaceKind, MapTriangle},
};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn has_area([a, b, c]: [Point; 3]) -> bool {
    let ab = std::array::from_fn::<_, 3, _>(|i| b[i] - a[i]);
    let ac = std::array::from_fn::<_, 3, _>(|i| c[i] - a[i]);
    // Voxel triangle edges are contained in one 500 mm cell; i128 also keeps
    // this exact for any supported canonical position accepted by the adapter.
    (0..3).any(|i| {
        i128::from(ab[(i + 1) % 3]) * i128::from(ac[(i + 2) % 3])
            != i128::from(ab[(i + 2) % 3]) * i128::from(ac[(i + 1) % 3])
    })
}

pub(crate) fn contours(
    triangles: &[MapTriangle],
    keys: &[TerrainChunkKey],
) -> Result<Vec<(Point, Point)>, ExcavationGeometryError> {
    let faces = exposed_faces(keys);
    let mut incidence = BTreeMap::<(Point, Point), u8>::new();
    for triangle in triangles {
        if !triangle.surface.is_some_and(|surface| {
            surface
                .iter()
                .all(|vertex| matches!(vertex.kind, MapSurfaceKind::Excavation))
        }) {
            continue;
        }
        let points = triangle
            .precise_vertices
            .ok_or(ExcavationGeometryError::UnavailableSurface)?;
        if !has_area(points) {
            return Err(ExcavationGeometryError::OpenVoxelContour);
        }
        for index in 0..3 {
            let (a, b) = (points[index], points[(index + 1) % 3]);
            let edge = if a < b { (a, b) } else { (b, a) };
            let count = incidence.entry(edge).or_default();
            *count += 1;
            if *count > 2 {
                return Err(ExcavationGeometryError::OpenVoxelContour);
            }
        }
    }
    let mut boundary = Vec::new();
    for (edge, count) in incidence {
        if count != 1 {
            continue;
        }
        // Every open mesh edge must be fully owned by an exposed domain face.
        // Never hide an internal hole by selecting only convenient face edges.
        if !faces.iter().any(|face| face.contains(edge)) {
            return Err(ExcavationGeometryError::OpenVoxelContour);
        }
        boundary.push(edge);
    }
    Ok(boundary)
}

struct Face {
    minimum: Point,
    maximum: Point,
    axis: usize,
    plane: i64,
}

impl Face {
    fn contains(&self, (a, b): (Point, Point)) -> bool {
        [a, b].into_iter().all(|point| {
            point[self.axis] == self.plane
                && (0..3).all(|axis| {
                    self.minimum[axis] <= point[axis] && point[axis] <= self.maximum[axis]
                })
        })
    }
}

fn exposed_faces(keys: &[TerrainChunkKey]) -> Vec<Face> {
    let active: BTreeSet<_> = keys.iter().copied().collect();
    let chunk_mm = (TERRAIN_CHUNK_M * 1000.0) as i64;
    let mut faces = Vec::new();
    for key in keys {
        let minimum = key.coordinates.map(|v| i64::from(v) * chunk_mm);
        let maximum = minimum.map(|v| v + chunk_mm);
        for axis in 0..3 {
            for positive in [false, true] {
                let mut neighbor = *key;
                neighbor.coordinates[axis] += if positive { 1 } else { -1 };
                if !active.contains(&neighbor) {
                    faces.push(Face {
                        minimum,
                        maximum,
                        axis,
                        plane: if positive {
                            maximum[axis]
                        } else {
                            minimum[axis]
                        },
                    });
                }
            }
        }
    }
    faces
}
