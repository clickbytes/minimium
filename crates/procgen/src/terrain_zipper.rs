//! Match and triangulate closed contours using one physical perimeter measure.
use crate::{terrain_contours::Point, ExcavationGeometryError};

pub(crate) fn zipper(
    a: &[Point],
    b: &[Point],
    base_cuts: &mut crate::terrain_edge_splits::Splits,
    volume_cuts: &mut crate::terrain_edge_splits::Splits,
) -> Result<Vec<([Point; 3], [bool; 3])>, ExcavationGeometryError> {
    if let Some(strip) = crate::terrain_face_strip::triangulate(a, b, base_cuts, volume_cuts)? {
        return Ok(strip);
    }
    let (a, b) = align(a, b)?;
    let mut triangles = Vec::with_capacity(a.points.len() + b.points.len());
    let (mut i, mut j) = (0, 0);
    while i < a.points.len() || j < b.points.len() {
        let next_a = a.next(i);
        let next_b = b.next(j);
        let pa = a.point(i);
        let pb = b.point(j);
        if next_a == next_b {
            let an = a.point(i + 1);
            let bn = b.point(j + 1);
            triangles.extend([
                ([pa, an, bn], [true, true, false]),
                ([pa, bn, pb], [true, false, false]),
            ]);
            i += 1;
            j += 1;
        } else if next_a < next_b {
            triangles.push(([pa, a.point(i + 1), pb], [true, true, false]));
            i += 1;
        } else {
            triangles.push(([pa, b.point(j + 1), pb], [true, false, false]));
            j += 1;
        }
    }
    // Retain the existing millimetre-area degeneracy rule. No new vertices are
    // inserted: base/voxel boundary endpoints and surface provenance stay exact.
    Ok(triangles
        .into_iter()
        .filter(|(triangle, _)| area2(*triangle) >= 1.0)
        .collect())
}

struct Perimeter {
    points: Vec<Point>,
    /// Cumulative edge lengths normalized to [0, 1], including the closing edge.
    positions: Vec<f64>,
}

impl Perimeter {
    fn new(points: Vec<Point>) -> Result<Self, ExcavationGeometryError> {
        if points.len() < 3 {
            return Err(ExcavationGeometryError::InvalidTransitionContour);
        }
        let mut positions = Vec::with_capacity(points.len() + 1);
        positions.push(0.0);
        let mut total = 0.0;
        for i in 0..points.len() {
            let edge = delta(points[(i + 1) % points.len()], points[i]);
            let length = dot(edge, edge).sqrt();
            let next = total + length;
            if !next.is_finite() || next <= total {
                return Err(ExcavationGeometryError::InvalidTransitionContour);
            }
            total = next;
            positions.push(total);
        }
        for position in &mut positions {
            *position /= total;
        }
        if positions.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(ExcavationGeometryError::InvalidTransitionContour);
        }
        Ok(Self { points, positions })
    }

    fn point(&self, index: usize) -> Point {
        self.points[index % self.points.len()]
    }

    fn next(&self, edge: usize) -> f64 {
        self.positions
            .get(edge + 1)
            .copied()
            .unwrap_or(f64::INFINITY)
    }

    fn sample(&self, edge: usize, position: f64, origin: Point) -> [f64; 3] {
        let t =
            (position - self.positions[edge]) / (self.positions[edge + 1] - self.positions[edge]);
        let start = delta(self.point(edge), origin);
        let direction = delta(self.point(edge + 1), self.point(edge));
        std::array::from_fn(|axis| start[axis] + direction[axis] * t)
    }
}

fn align(a: &[Point], b: &[Point]) -> Result<(Perimeter, Perimeter), ExcavationGeometryError> {
    let start = a
        .iter()
        .enumerate()
        .min_by_key(|(_, p)| *p)
        .map_or(0, |(i, _)| i);
    let a = Perimeter::new((0..a.len()).map(|i| a[(start + i) % a.len()]).collect())?;
    // Validate b before trying rotations, so malformed input cannot leave an
    // empty candidate. Each candidate uses the complete perimeter, not zip's
    // truncated prefix when the base and Marching Cubes point counts differ.
    let initial = Perimeter::new(b.to_vec())?;
    let mut best_score = alignment_cost(&a, &initial);
    let mut best = initial;
    for reverse in [false, true] {
        for offset in 0..b.len() {
            if !reverse && offset == 0 {
                continue;
            }
            let points = (0..b.len())
                .map(|i| {
                    b[if reverse {
                        (offset + b.len() - i) % b.len()
                    } else {
                        (offset + i) % b.len()
                    }]
                })
                .collect();
            let candidate = Perimeter::new(points)?;
            let score = alignment_cost(&a, &candidate);
            if score < best_score {
                best_score = score;
                best = candidate;
            }
        }
    }
    Ok((a, best))
}

/// Integral of squared separation over normalized arc length. On each interval
/// between either contour's vertices, separation varies linearly; integrating
/// its square exactly avoids weighting a densely tessellated edge more heavily.
fn alignment_cost(a: &Perimeter, b: &Perimeter) -> f64 {
    let (mut i, mut j) = (0, 0);
    let mut position = 0.0;
    let mut cost = 0.0;
    let origin = a.point(0);
    while i < a.points.len() && j < b.points.len() {
        let next = a.next(i).min(b.next(j));
        let separation = |position| {
            let pa = a.sample(i, position, origin);
            let pb = b.sample(j, position, origin);
            std::array::from_fn(|axis| pa[axis] - pb[axis])
        };
        let start = separation(position);
        let end = separation(next);
        cost += (next - position) * (dot(start, start) + dot(start, end) + dot(end, end)) / 3.0;
        position = next;
        if a.next(i) == next {
            i += 1;
        }
        if b.next(j) == next {
            j += 1;
        }
    }
    cost
}

fn delta(a: Point, b: Point) -> [f64; 3] {
    std::array::from_fn(|axis| a[axis] as f64 - b[axis] as f64)
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|axis| a[axis] * b[axis]).sum()
}

fn area2([a, b, c]: [Point; 3]) -> f64 {
    let ab = delta(b, a);
    let ac = delta(c, a);
    let cross = [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ];
    dot(cross, cross)
}
