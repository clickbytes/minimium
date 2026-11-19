//! Canonical splits shared by the planar seam and the triangles it joins.
use crate::{sector_clip, terrain_contours::Point, ExcavationGeometryError};
use minimium_world::world::MapTriangle;
use std::collections::BTreeMap;

pub(crate) type Edge = (Point, Point);
pub(crate) fn ordered(a: Point, b: Point) -> Edge {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

#[derive(Default)]
pub(crate) struct Splits(pub BTreeMap<Edge, Vec<Point>>);

impl Splits {
    pub fn insert(&mut self, a: Point, b: Point, p: Point) {
        if p != a && p != b {
            self.0.entry(ordered(a, b)).or_default().push(p);
        }
    }

    pub fn path(&self, path: &[Point]) -> Vec<Point> {
        let mut result = Vec::new();
        for w in path.windows(2) {
            result.push(w[0]);
            if let Some(points) = self.0.get(&ordered(w[0], w[1])) {
                result.extend(sort_points(w[0], w[1], points.clone()));
            }
        }
        result.extend(path.last());
        result
    }
}

fn sort_points(a: Point, b: Point, mut points: Vec<Point>) -> Vec<Point> {
    points.sort_unstable_by_key(|p| {
        (
            (0..3)
                .map(|i| i128::from(p[i] - a[i]) * i128::from(b[i] - a[i]))
                .sum::<i128>(),
            *p,
        )
    });
    points.dedup();
    points
}

/// Sequential clipping may leave a longer or shorter edge on this source
/// triangle's intersection line. Match within the rounding of both endpoints,
/// while requiring the exact same chunk plane and an interior projected split.
pub(crate) fn on_clipped_edge(a: Point, b: Point, e: Point, f: Point, p: Point) -> bool {
    if !(0..3).any(|i| a[i] == b[i] && a[i] == e[i] && e[i] == f[i] && e[i] == p[i]) {
        return false;
    }
    let project = |x: Point, y: Point, q: Point| {
        let d = std::array::from_fn::<_, 3, _>(|i| (y[i] - x[i]) as f64);
        let v = std::array::from_fn::<_, 3, _>(|i| (q[i] - x[i]) as f64);
        let length2 = d.iter().map(|v| v * v).sum::<f64>();
        let t = d.iter().zip(v).map(|(a, b)| a * b).sum::<f64>() / length2;
        (
            t,
            (0..3).map(|i| (v[i] - t * d[i]).powi(2)).sum::<f64>(),
            length2,
        )
    };
    let (t, distance2, ab2) = project(a, b, p);
    let (_, _, ef2) = project(e, f, p);
    let (long, short) = if ab2 >= ef2 {
        ([a, b], [e, f])
    } else {
        ([e, f], [a, b])
    };
    0.0 < t
        && t < 1.0
        && distance2 <= 2.0
        && short.iter().all(|&q| project(long[0], long[1], q).1 <= 2.0)
}

/// Pin the ends of the contributing face span too. Its rounded crossing bends
/// E-P-F, so adding P alone to a longer clipped edge A-B would leave a crack.
/// Inspect only the polygon's perimeter, before its fan introduces diagonals.
pub(crate) fn clipped_polygon_splits(
    polygon: &[[f64; 3]],
    cuts: &[(Edge, &Vec<Point>)],
) -> Result<Splits, ExcavationGeometryError> {
    let mut result = Splits::default();
    if cuts.is_empty() {
        return Ok(result);
    }
    for i in 0..polygon.len() {
        let [a, b] =
            [polygon[i], polygon[(i + 1) % polygon.len()]].map(|p| p.map(|v| v.round() as i64));
        if a == b {
            continue;
        }
        for &((e, f), points) in cuts {
            for q in [a, b] {
                // An independently rounded intermediate vertex on this
                // fragment must also be an explicit vertex of the seam chain.
                if q != e && q != f && !points.contains(&q) && on_clipped_edge(e, f, a, b, q) {
                    return Err(ExcavationGeometryError::InvalidTransitionContour);
                }
            }
            for p in [e, f].into_iter().chain(points.iter().copied()) {
                if p != a && p != b && on_clipped_edge(a, b, e, f, p) {
                    result.insert(a, b, p);
                }
            }
        }
    }
    Ok(result)
}

fn normal([a, b, c]: [Point; 3]) -> [f64; 3] {
    std::array::from_fn(|i| {
        let (j, k) = ((i + 1) % 3, (i + 2) % 3);
        (b[j] - a[j]) as f64 * (c[k] - a[k]) as f64 - (b[k] - a[k]) as f64 * (c[j] - a[j]) as f64
    })
}

/// Splits one original source edge at a time. Only the triangle incident to
/// the next original edge is subdivided again in a multi-edge corner case.
pub(crate) fn subdivide(
    source: MapTriangle,
    mut cuts: impl FnMut(Point, Point) -> Vec<Point>,
    output: &mut Vec<MapTriangle>,
) -> Result<(), ExcavationGeometryError> {
    let (Some(points), Some(_)) = (source.precise_vertices, source.surface) else {
        output.push(source);
        return Ok(());
    };
    let first = output.len();
    output.push(source);
    for i in 0..3 {
        let (a, b) = (points[i], points[(i + 1) % 3]);
        let splits = sort_points(a, b, cuts(a, b));
        if splits.is_empty() {
            continue;
        }
        let (index, edge) = output[first..]
            .iter()
            .enumerate()
            .find_map(|(k, tri)| {
                let p = tri.precise_vertices?;
                (0..3)
                    .find(|&j| p[j] == a && p[(j + 1) % 3] == b)
                    .map(|j| (first + k, j))
            })
            .ok_or(ExcavationGeometryError::InvalidTransitionContour)?;
        let tri = output.swap_remove(index);
        let tri_points = tri.precise_vertices.unwrap();
        let surface = tri.surface.unwrap();
        let next = (edge + 1) % 3;
        let opposite = (edge + 2) % 3;
        let axis = (0..3).max_by_key(|&j| (b[j] - a[j]).abs()).unwrap();
        let original_normal = normal(tri_points);
        let (mut previous, mut metadata) = (a, surface[edge]);
        for p in splits.into_iter().chain([b]) {
            let new_points = [previous, p, tri_points[opposite]];
            // A nearest-mm crossing can fall beyond a very thin triangle's
            // opposite vertex. Such a split must not flip or fold the mesh.
            if normal(new_points)
                .iter()
                .zip(original_normal)
                .map(|(a, b)| a * b)
                .sum::<f64>()
                <= 0.0
            {
                return Err(ExcavationGeometryError::InvalidTransitionContour);
            }
            let t = (p[axis] - a[axis]) as f64 / (b[axis] - a[axis]) as f64;
            let at_p = if p == b {
                surface[next]
            } else {
                sector_clip::interpolate(surface[edge], surface[next], t as f32)
            };
            output.push(MapTriangle {
                precise_vertices: Some(new_points),
                vertices: new_points.map(|p| p.map(|v| v as f32 / 1000.0)),
                surface: Some([metadata, at_p, surface[opposite]]),
                ..tri
            });
            (previous, metadata) = (p, at_p);
        }
    }
    Ok(())
}
