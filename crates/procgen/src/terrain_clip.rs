//! Exact polygon subtraction against activated voxel chunk boxes.
pub type Point = [f64; 3];
pub type Polygon = Vec<Point>;

#[derive(Clone, Copy, Debug)]
pub struct Box3 {
    pub minimum: Point,
    pub maximum: Point,
}

pub fn subtract_boxes(mut polygons: Vec<Polygon>, boxes: &[Box3]) -> Vec<Polygon> {
    for bounds in boxes {
        polygons = polygons
            .into_iter()
            .flat_map(|polygon| subtract_box(polygon, *bounds))
            .collect();
    }
    polygons
}

/// Clips one triangle against the complement of an axis-aligned box set.
/// Each box is `[minimum, maximum]`; output polygons preserve triangle order.
pub fn clip_triangle_outside_boxes(
    triangle: [[f64; 3]; 3],
    boxes: &[[[f64; 3]; 2]],
) -> Vec<Polygon> {
    let boxes: Vec<_> = boxes
        .iter()
        .map(|bounds| Box3 {
            minimum: bounds[0],
            maximum: bounds[1],
        })
        .collect();
    subtract_boxes(vec![triangle.to_vec()], &boxes)
}

fn subtract_box(polygon: Polygon, bounds: Box3) -> Vec<Polygon> {
    let mut pending = vec![polygon];
    let mut outside = Vec::new();
    for axis in 0..3 {
        for bound in [bounds.minimum[axis], bounds.maximum[axis]] {
            let keep_le = bound == bounds.maximum[axis];
            let mut next = Vec::new();
            for candidate in pending {
                let (inside, rejected) = split(candidate, axis, bound, keep_le);
                if rejected.len() >= 3 {
                    outside.push(rejected);
                }
                if inside.len() >= 3 {
                    next.push(inside);
                }
            }
            pending = next;
        }
    }
    outside
}

/// Retains the closed polygon inside a box, including coplanar face polygons.
pub(crate) fn clip_inside_box(mut polygon: Polygon, bounds: Box3) -> Polygon {
    for axis in 0..3 {
        for (bound, keep_le) in [(bounds.minimum[axis], false), (bounds.maximum[axis], true)] {
            polygon = split(polygon, axis, bound, keep_le).0;
            if polygon.len() < 3 {
                return Vec::new();
            }
        }
    }
    polygon
}

fn split(polygon: Polygon, axis: usize, bound: f64, keep_le: bool) -> (Polygon, Polygon) {
    let inside = |point: Point| {
        if keep_le {
            point[axis] <= bound
        } else {
            point[axis] >= bound
        }
    };
    let mut kept = Vec::new();
    let mut rejected = Vec::new();
    for index in 0..polygon.len() {
        let a = polygon[index];
        let b = polygon[(index + 1) % polygon.len()];
        let a_inside = inside(a);
        let b_inside = inside(b);
        if a_inside {
            kept.push(a);
        } else {
            rejected.push(a);
        }
        if a_inside != b_inside {
            let t = ((bound - a[axis]) / (b[axis] - a[axis])).clamp(0.0, 1.0);
            let mut crossing = std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t);
            crossing[axis] = bound;
            kept.push(crossing);
            rejected.push(crossing);
        }
    }
    (dedupe(kept), dedupe(rejected))
}

fn dedupe(mut polygon: Polygon) -> Polygon {
    polygon.dedup_by(|a, b| (0..3).all(|axis| (a[axis] - b[axis]).abs() <= 1.0e-7));
    if polygon.len() > 1
        && (0..3).all(|axis| (polygon[0][axis] - polygon[polygon.len() - 1][axis]).abs() <= 1.0e-7)
    {
        polygon.pop();
    }
    polygon
}
