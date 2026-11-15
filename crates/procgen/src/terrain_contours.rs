//! Canonical closed contour graph and deterministic perimeter zipper.
use std::collections::{BTreeMap, BTreeSet};

pub(crate) type Point = [i64; 3];
pub(crate) type Loop = Vec<Point>;

pub(crate) fn closed_loops(segments: &[(Point, Point)]) -> Result<Vec<Loop>, &'static str> {
    let mut edges = BTreeSet::new();
    for &(a, b) in segments {
        if a == b {
            continue;
        }
        edges.insert(if a < b { (a, b) } else { (b, a) });
    }
    let mut adjacent: BTreeMap<Point, Vec<Point>> = BTreeMap::new();
    for &(a, b) in &edges {
        adjacent.entry(a).or_default().push(b);
        adjacent.entry(b).or_default().push(a);
    }
    if adjacent.values().any(|neighbors| neighbors.len() != 2) {
        return Err("terrain contour has an open or branching endpoint");
    }
    let mut remaining = edges;
    let mut loops = Vec::new();
    while let Some(&(start, first)) = remaining.iter().next() {
        let mut points = vec![start];
        let mut previous = start;
        let mut current = first;
        remaining.remove(&(start, first));
        while current != start {
            if points.len() > adjacent.len() {
                return Err("terrain contour walk did not close");
            }
            points.push(current);
            let neighbors = adjacent
                .get(&current)
                .ok_or("terrain contour endpoint is missing")?;
            let next = if neighbors[0] == previous {
                neighbors[1]
            } else {
                neighbors[0]
            };
            let edge = if current < next {
                (current, next)
            } else {
                (next, current)
            };
            if next != start && !remaining.remove(&edge) {
                return Err("terrain contour contains a repeated edge");
            }
            if next == start {
                remaining.remove(&edge);
            }
            previous = current;
            current = next;
        }
        if points.len() < 3 {
            return Err("terrain contour is degenerate");
        }
        loops.push(points);
    }
    loops.sort_by_key(|path| *path.iter().min().unwrap());
    Ok(loops)
}

pub(crate) fn zipper(
    a: &[Point],
    b: &[Point],
) -> Result<Vec<([Point; 3], [bool; 3])>, &'static str> {
    if a.len() < 3 || b.len() < 3 {
        return Err("transition contour has fewer than three vertices");
    }
    let (a, b) = align(a, b);
    let mut triangles = Vec::with_capacity(a.len() + b.len());
    let (mut i, mut j) = (0, 0);
    while i < a.len() || j < b.len() {
        let next_a = (i + 1) as f64 / a.len() as f64;
        let next_b = (j + 1) as f64 / b.len() as f64;
        let pa = a[i % a.len()];
        let pb = b[j % b.len()];
        if (next_a - next_b).abs() < 1.0e-12 {
            let an = a[(i + 1) % a.len()];
            let bn = b[(j + 1) % b.len()];
            let first = [pa, an, bn];
            let second = [pa, bn, pb];
            triangles.extend([(first, [true, true, false]), (second, [true, false, false])]);
            i += 1;
            j += 1;
        } else if next_a < next_b {
            triangles.push(([pa, a[(i + 1) % a.len()], pb], [true, true, false]));
            i += 1;
        } else {
            triangles.push(([pa, b[(j + 1) % b.len()], pb], [true, false, false]));
            j += 1;
        }
    }
    Ok(triangles
        .into_iter()
        .filter(|(triangle, _)| area2(*triangle) >= 1.0)
        .collect())
}

/// Rejects intersections between transition strips before geometry can be
/// published. Shared vertices are expected at canonical chunk-face corners.
pub(crate) fn validate_strip_intersections(
    strips: &[Vec<([Point; 3], [bool; 3])>],
) -> Result<(), &'static str> {
    for strip in strips {
        for first in 0..strip.len() {
            for second in first + 1..strip.len() {
                if shared_vertices(&strip[first].0, &strip[second].0) > 1 {
                    continue;
                }
                if triangles_intersect(strip[first].0, strip[second].0) {
                    return Err("transition strip self-intersects");
                }
            }
        }
    }
    for first in 0..strips.len() {
        for second in first + 1..strips.len() {
            for (a, _) in &strips[first] {
                for (b, _) in &strips[second] {
                    if shared_vertices(a, b) > 1 {
                        continue;
                    }
                    if triangles_intersect(*a, *b) {
                        return Err("transition strips intersect");
                    }
                }
            }
        }
    }
    Ok(())
}

fn shared_vertices(a: &[Point; 3], b: &[Point; 3]) -> usize {
    a.iter().filter(|point| b.contains(point)).count()
}

fn triangles_intersect(a: [Point; 3], b: [Point; 3]) -> bool {
    if !(0..3).all(|axis| {
        a.iter().map(|p| p[axis]).min().unwrap() <= b.iter().map(|p| p[axis]).max().unwrap()
            && b.iter().map(|p| p[axis]).min().unwrap() <= a.iter().map(|p| p[axis]).max().unwrap()
    }) {
        return false;
    }
    if (0..3).any(|i| segment_hits_triangle(a[i], a[(i + 1) % 3], b))
        || (0..3).any(|i| segment_hits_triangle(b[i], b[(i + 1) % 3], a))
    {
        return true;
    }
    coplanar_triangles_overlap(a, b)
}

fn segment_hits_triangle(start: Point, end: Point, triangle: [Point; 3]) -> bool {
    let origin = start.map(|v| v as f64);
    let direction = std::array::from_fn::<_, 3, _>(|i| (end[i] - start[i]) as f64);
    let a = triangle[0].map(|v| v as f64);
    let edge1 = subf(triangle[1].map(|v| v as f64), a);
    let edge2 = subf(triangle[2].map(|v| v as f64), a);
    let h = crossf(direction, edge2);
    let determinant = dotf(edge1, h);
    if determinant.abs() < 1.0e-9 {
        return false;
    }
    let offset = subf(origin, a);
    let u = dotf(offset, h) / determinant;
    if !(0.0..=1.0).contains(&u) {
        return false;
    }
    let q = crossf(offset, edge1);
    let v = dotf(direction, q) / determinant;
    if v < 0.0 || u + v > 1.0 {
        return false;
    }
    let t = dotf(edge2, q) / determinant;
    t > 1.0e-8 && t < 1.0 - 1.0e-8
}

fn coplanar_triangles_overlap(a: [Point; 3], b: [Point; 3]) -> bool {
    let a3 = a.map(|p| p.map(|v| v as f64));
    let b3 = b.map(|p| p.map(|v| v as f64));
    let normal = crossf(subf(a3[1], a3[0]), subf(a3[2], a3[0]));
    let other = crossf(subf(b3[1], b3[0]), subf(b3[2], b3[0]));
    let parallel = crossf(normal, other).iter().map(|v| v * v).sum::<f64>() < 1.0e-12;
    if !parallel || dotf(normal, subf(b3[0], a3[0])).abs() > 1.0e-4 {
        return false;
    }
    let axis = (0..3)
        .max_by(|i, j| normal[*i].abs().total_cmp(&normal[*j].abs()))
        .unwrap();
    let axes: Vec<_> = (0..3).filter(|i| *i != axis).collect();
    let project = |p: [f64; 3]| [p[axes[0]], p[axes[1]]];
    let a2 = a3.map(project);
    let b2 = b3.map(project);
    (0..3).any(|i| (0..3).any(|j| edges_overlap(a2[i], a2[(i + 1) % 3], b2[j], b2[(j + 1) % 3])))
        || point_in_triangle(a2[0], b2)
        || point_in_triangle(b2[0], a2)
}

fn edges_overlap(a: [f64; 2], b: [f64; 2], c: [f64; 2], d: [f64; 2]) -> bool {
    let orient = |p: [f64; 2], q: [f64; 2], r: [f64; 2]| {
        (q[0] - p[0]) * (r[1] - p[1]) - (q[1] - p[1]) * (r[0] - p[0])
    };
    let (ab_c, ab_d, cd_a, cd_b) = (
        orient(a, b, c),
        orient(a, b, d),
        orient(c, d, a),
        orient(c, d, b),
    );
    ab_c * ab_d < -1.0e-8
        && cd_a * cd_b < -1.0e-8
        && (0..2).all(|axis| {
            a[axis].min(b[axis]) - 1.0e-4 <= c[axis].max(d[axis])
                && c[axis].min(d[axis]) - 1.0e-4 <= a[axis].max(b[axis])
        })
}

fn point_in_triangle(point: [f64; 2], triangle: [[f64; 2]; 3]) -> bool {
    let orient = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| {
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    };
    let a = orient(triangle[0], triangle[1], point);
    let b = orient(triangle[1], triangle[2], point);
    let c = orient(triangle[2], triangle[0], point);
    (a > 1.0e-4 && b > 1.0e-4 && c > 1.0e-4) || (a < -1.0e-4 && b < -1.0e-4 && c < -1.0e-4)
}

fn subf(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}

fn dotf(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| a[i] * b[i]).sum()
}

fn crossf(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn align(a: &[Point], b: &[Point]) -> (Vec<Point>, Vec<Point>) {
    let a = rotate_min(a);
    let mut best = (f64::INFINITY, Vec::new());
    for reverse in [false, true] {
        let candidate = if reverse {
            b.iter().copied().rev().collect::<Vec<_>>()
        } else {
            b.to_vec()
        };
        for offset in 0..candidate.len() {
            let rotated = (0..candidate.len())
                .map(|i| candidate[(i + offset) % candidate.len()])
                .collect::<Vec<_>>();
            let score = a.iter().zip(&rotated).map(|(x, y)| distance2(*x, *y)).sum();
            if score < best.0 {
                best = (score, rotated);
            }
        }
    }
    (a, best.1)
}

fn rotate_min(points: &[Point]) -> Vec<Point> {
    let start = points
        .iter()
        .enumerate()
        .min_by_key(|(_, p)| *p)
        .map_or(0, |(i, _)| i);
    (0..points.len())
        .map(|i| points[(i + start) % points.len()])
        .collect()
}

fn distance2(a: Point, b: Point) -> f64 {
    (0..3).map(|i| (a[i] as f64 - b[i] as f64).powi(2)).sum()
}

fn area2([a, b, c]: [Point; 3]) -> f64 {
    let ab = std::array::from_fn::<_, 3, _>(|i| b[i] as f64 - a[i] as f64);
    let ac = std::array::from_fn::<_, 3, _>(|i| c[i] as f64 - a[i] as f64);
    let cross = [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ];
    cross.iter().map(|v| v * v).sum()
}
