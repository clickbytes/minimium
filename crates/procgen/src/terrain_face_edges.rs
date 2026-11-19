//! Integer segment predicates and boundary certification in face-local millimetres.
use crate::{terrain_contours::Point, terrain_edge_splits::Splits, terrain_face_strip::Face};
use std::collections::BTreeSet;

type XY = [i64; 2];
fn cross(a: XY, b: XY) -> i64 {
    a[0] * b[1] - a[1] * b[0]
}
fn sub(a: XY, b: XY) -> XY {
    [a[0] - b[0], a[1] - b[1]]
}
fn between(p: XY, a: XY, b: XY) -> bool {
    (0..2).all(|i| a[i].min(b[i]) <= p[i] && p[i] <= a[i].max(b[i]))
}
fn xy(face: Face, p: Point) -> XY {
    let p = face.project(p);
    [i64::from(p.x), i64::from(p.y)]
}

// Returns endpoints in a shared interval (including a single contact), or the
// common rounded vertex of a proper crossing. Local 16 m bounds keep products
// within i64. The absolute numerator makes rounding independent of winding.
fn crossing(face: Face, first: &[Point], second: &[Point]) -> Vec<Point> {
    let [a, b, c, d] = [first[0], first[1], second[0], second[1]].map(|p| xy(face, p));
    let ab = sub(b, a);
    let cd = sub(d, c);
    let det = cross(ab, cd);
    let ca = sub(c, a);
    if det == 0 {
        if cross(ab, ca) != 0 {
            return Vec::new();
        }
        return [first[0], first[1], second[0], second[1]]
            .into_iter()
            .filter(|&p| {
                let p = xy(face, p);
                between(p, a, b) && between(p, c, d)
            })
            .collect();
    }
    let (t, u) = (cross(ca, cd) * det.signum(), cross(ca, ab) * det.signum());
    let den = det.abs();
    if !(0..=den).contains(&t) || !(0..=den).contains(&u) {
        return Vec::new();
    }
    let axis = |j: usize| ((a[j] * den + t * ab[j] + den / 2) / den) as i32;
    vec![
        face.lift(i_triangle::i_overlay::i_float::int::point::IntPoint::new(
            axis(0),
            axis(1),
        )),
    ]
}

pub(super) fn split_crossings(face: Face, a: &[Point], b: &[Point], cuts: &mut Splits) {
    for first in a.windows(2) {
        for second in b.windows(2) {
            for p in crossing(face, first, second) {
                cuts.insert(first[0], first[1], p);
                cuts.insert(second[0], second[1], p);
            }
        }
    }
}

pub(super) fn source_is_simple(face: Face, edges: &[(Point, Point)]) -> bool {
    edges.iter().enumerate().all(|(i, &(a, b))| {
        edges[i + 1..].iter().all(|&(c, d)| {
            crossing(face, &[a, b], &[c, d])
                .into_iter()
                .all(|p| (p == a || p == b) && (p == c || p == d))
        })
    })
}

/// The mod-two boundary of the triangulation must equal the requested chains.
/// Splitting an edge into collinear pieces adds two copies of each interior
/// endpoint, so the parity key below is invariant under retessellation.
#[derive(Default)]
pub(super) struct Boundary(BTreeSet<([i64; 3], XY)>);

impl Boundary {
    pub fn edge(&mut self, face: Face, a: Point, b: Point) {
        if a == b {
            return;
        }
        let [a, b] = [a, b].map(|p| xy(face, p));
        let [mut dx, mut dy] = sub(b, a);
        let (mut u, mut v) = (dx.abs(), dy.abs());
        while v != 0 {
            (u, v) = (v, u % v);
        }
        let sign = if dx < 0 || (dx == 0 && dy < 0) { -1 } else { 1 };
        dx = dx / u * sign;
        dy = dy / u * sign;
        let line = [dx, dy, dx * a[1] - dy * a[0]];
        for p in [a, b] {
            if !self.0.insert((line, p)) {
                self.0.remove(&(line, p));
            }
        }
    }
    pub fn complete(&self) -> bool {
        self.0.is_empty()
    }
}
