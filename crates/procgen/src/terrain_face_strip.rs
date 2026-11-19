//! Triangulate the area between coarse and fine contour chains on a shared chunk face.
use crate::{
    terrain_contours::Point,
    terrain_edge_splits::Splits,
    terrain_face_edges::{self, Boundary},
    ExcavationGeometryError,
};
use i_triangle::{
    i_overlay::{core::fill_rule::FillRule, i_float::int::point::IntPoint},
    int::{triangulator::IntTriangulator, validation::Validation},
};
use minimium_world::terrain::TERRAIN_CHUNK_M;
use std::collections::{BTreeMap, BTreeSet};

const CHUNK_MM: i64 = (TERRAIN_CHUNK_M * 1000.0) as i64;

#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
pub(super) struct Face {
    axis: usize,
    origin: Point,
}

impl Face {
    fn of_edge(a: Point, b: Point) -> Option<Self> {
        // A segment along a box edge belongs to two planes. Leave this
        // ambiguous case to the checked spatial zipper.
        let mut axes = (0..3).filter(|&i| a[i] == b[i] && a[i].rem_euclid(CHUNK_MM) == 0);
        let axis = axes.next()?;
        if axes.next().is_some() {
            return None;
        }
        let origin = std::array::from_fn(|i| (a[i] + b[i]).div_euclid(2 * CHUNK_MM) * CHUNK_MM);
        [a, b]
            .iter()
            .all(|p| (0..3).all(|i| origin[i] <= p[i] && p[i] <= origin[i] + CHUNK_MM))
            .then_some(Self { axis, origin })
    }

    pub(super) fn project(self, p: Point) -> IntPoint<i32> {
        let x = (self.axis + 1) % 3;
        let y = (self.axis + 2) % 3;
        // Coordinates are local to one 16 m face, in integer millimetres.
        IntPoint::new(
            (p[x] - self.origin[x]) as i32,
            (p[y] - self.origin[y]) as i32,
        )
    }

    pub(super) fn lift(self, p: IntPoint<i32>) -> Point {
        let mut world = self.origin;
        world[(self.axis + 1) % 3] += i64::from(p.x);
        world[(self.axis + 2) % 3] += i64::from(p.y);
        world
    }
}

struct Span {
    face: Face,
    points: Vec<Point>,
}

fn spans(path: &[Point]) -> Option<Vec<Span>> {
    if path.len() < 3 {
        return None;
    }
    let count = path.len();
    let faces: Vec<_> = (0..count)
        .map(|i| Face::of_edge(path[i], path[(i + 1) % count]))
        .collect::<Option<_>>()?;
    let start = (0..count)
        .find(|&i| faces[i] != faces[(i + count - 1) % count])
        .unwrap_or(0);
    let mut result = Vec::<Span>::new();
    for offset in 0..count {
        let i = (start + offset) % count;
        let face = faces[i];
        if result.last().is_none_or(|span| span.face != face) {
            result.push(Span {
                face,
                points: vec![path[i]],
            });
        }
        result.last_mut()?.points.push(path[(i + 1) % count]);
    }
    Some(result)
}

/// Corresponding runs must traverse the same actual face rectangle and join
/// on the same box edges. Score endpoints only to disambiguate repeated runs.
fn align(base: &[Span], volume: &[Span]) -> Option<(usize, bool)> {
    if base.len() != volume.len() {
        return None;
    }
    let n = base.len();
    let mut best = None;
    for reverse in [false, true] {
        for offset in 0..n {
            let mut cost = 0_i128;
            for (i, a) in base.iter().enumerate() {
                let b = &volume[span_index(n, i, offset, reverse)];
                if a.face != b.face {
                    break;
                }
                let b_start = if reverse {
                    b.points.last()?
                } else {
                    &b.points[0]
                };
                for (x, y) in a.points[0].into_iter().zip(b_start) {
                    cost += i128::from(x - y).pow(2);
                }
                if i + 1 == n && best.is_none_or(|(previous, _, _)| cost < previous) {
                    best = Some((cost, offset, reverse));
                }
            }
        }
    }
    best.map(|(_, offset, reverse)| (offset, reverse))
}

fn span_index(n: usize, i: usize, offset: usize, reverse: bool) -> usize {
    (offset + if reverse { n - i } else { i }) % n
}

fn validate_source(spans: &[Span], splits: &Splits) -> Result<(), ExcavationGeometryError> {
    let mut faces = BTreeMap::<Face, Vec<_>>::new();
    let mut vertices = BTreeSet::new();
    for span in spans {
        let points = splits.path(&span.points);
        for edge in points.windows(2) {
            if !vertices.insert(edge[0]) {
                return Err(ExcavationGeometryError::InvalidTransitionContour);
            }
            faces.entry(span.face).or_default().push((edge[0], edge[1]));
        }
    }
    if faces
        .iter()
        .all(|(&face, edges)| terrain_face_edges::source_is_simple(face, edges))
    {
        Ok(())
    } else {
        Err(ExcavationGeometryError::InvalidTransitionContour)
    }
}

pub(crate) fn triangulate(
    base: &[Point],
    volume: &[Point],
    base_cuts: &mut Splits,
    volume_cuts: &mut Splits,
) -> Result<Option<Vec<([Point; 3], [bool; 3])>>, ExcavationGeometryError> {
    let (Some(base), Some(volume)) = (spans(base), spans(volume)) else {
        return Ok(None);
    };
    let Some((offset, reverse)) = align(&base, &volume) else {
        return Ok(None);
    };
    let mut cuts = Splits::default();
    validate_source(&base, &cuts)?;
    validate_source(&volume, &cuts)?;
    for (i, a) in base.iter().enumerate() {
        let b = &volume[span_index(volume.len(), i, offset, reverse)];
        terrain_face_edges::split_crossings(a.face, &a.points, &b.points, &mut cuts);
    }
    if !cuts.0.is_empty() {
        validate_source(&base, &cuts)?;
        validate_source(&volume, &cuts)?;
    }
    let mut triangles = Vec::new();
    let mut validation = Validation::with_fill_rule(FillRule::EvenOdd);
    validation.options.preserve_input_collinear = true;
    let mut solver = IntTriangulator::<i32, usize>::new(0, validation, Default::default());
    for (i, a) in base.iter().enumerate() {
        let b = &volume[span_index(volume.len(), i, offset, reverse)];
        let a_points = cuts.path(&a.points);
        let b_points = cuts.path(&b.points);
        let base_points: BTreeSet<_> = a_points.iter().copied().collect();
        let mut boundary = Boundary::default();
        for chain in [&a_points, &b_points] {
            for edge in chain.windows(2) {
                boundary.edge(a.face, edge[0], edge[1]);
            }
        }
        let project = |p: &Point| a.face.project(*p);
        let mut path: Vec<_> = a_points.iter().map(project).collect();
        let mesh = if base.len() == 1 {
            // Both contours close within this same face. XOR their interiors,
            // including a possible central hole.
            path.pop();
            let second = b_points[..b_points.len() - 1].iter().map(project).collect();
            solver.triangulate_shape(&vec![path, second])
        } else {
            // Walk the old chain forwards and its matched voxel chain backwards.
            // Connectors between them run along shared box edges. The even-odd
            // fill splits true crossings and cancels coincident boundary spans.
            if reverse {
                path.extend(b_points.iter().map(project));
                boundary.edge(a.face, a_points[0], *b_points.last().unwrap());
                boundary.edge(a.face, *a_points.last().unwrap(), b_points[0]);
            } else {
                path.extend(b_points.iter().rev().map(project));
                boundary.edge(a.face, a_points[0], b_points[0]);
                boundary.edge(a.face, *a_points.last().unwrap(), *b_points.last().unwrap());
            }
            solver.triangulate_contour(&path)
        };
        for triangle in mesh.triangles() {
            let points = triangle.map(|p| a.face.lift(p));
            if crate::voxel_boundary::has_area(points) {
                for j in 0..3 {
                    boundary.edge(a.face, points[j], points[(j + 1) % 3]);
                }
                triangles.push((points, points.map(|p| base_points.contains(&p))));
            }
        }
        if !boundary.complete() {
            return Err(ExcavationGeometryError::InvalidTransitionContour);
        }
    }
    for (source, destinations) in [(&base, base_cuts), (&volume, volume_cuts)] {
        for span in source {
            for e in span.points.windows(2) {
                if let Some(points) = cuts.0.get(&crate::terrain_edge_splits::ordered(e[0], e[1])) {
                    for &p in points {
                        destinations.insert(e[0], e[1], p);
                    }
                }
            }
        }
    }
    Ok(Some(triangles))
}
