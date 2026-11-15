use crate::{FluidSnapshot, CELL_SIZE_M};

const SAMPLES: i32 = 6;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Plane {
    pub(crate) normal: [f64; 3],
    pub(crate) offset: f64,
}

/// Reconstructs a Youngs-gradient interface plane and estimates its face
/// aperture by deterministic midpoint quadrature. The local phase volume is
/// matched by cube quadrature and a bounded bisection of plane offset.
pub(crate) fn reconstruct(snapshot: &FluidSnapshot, key: [i32; 3]) -> Option<Plane> {
    let Some(cell) = snapshot.cells().get(&key) else {
        return None;
    };
    if cell.capacity_m3 <= 0.0 || cell.volume_m3 <= 0.0 || cell.volume_m3 >= cell.capacity_m3 {
        return None;
    }
    let theta = (cell.volume_m3 / cell.capacity_m3).clamp(0.0, 1.0);
    let mut normal = [0.0; 3];
    for direction in 0..3 {
        let mut low = key;
        low[direction] = low[direction].saturating_sub(1);
        let mut high = key;
        high[direction] = high[direction].saturating_add(1);
        let a = snapshot
            .cells()
            .get(&low)
            .map_or(theta, |c| c.volume_m3 / c.capacity_m3.max(1e-12));
        let b = snapshot
            .cells()
            .get(&high)
            .map_or(theta, |c| c.volume_m3 / c.capacity_m3.max(1e-12));
        // The liquid fraction gradient points into the liquid; the plane's
        // `dot(normal, point) <= offset` convention points toward air instead.
        normal[direction] = (a - b).clamp(-1.0, 1.0);
    }
    let length = normal.iter().map(|v| v * v).sum::<f64>().sqrt();
    if length < 1.0e-8 {
        let center = key.map(|v| f64::from(v) * CELL_SIZE_M + CELL_SIZE_M * 0.5);
        let radius = center.iter().map(|v| v * v).sum::<f64>().sqrt();
        normal = if radius > f64::EPSILON {
            center.map(|v| v / radius)
        } else {
            [0.0, 1.0, 0.0]
        };
    } else {
        for n in &mut normal {
            *n /= length;
        }
    }
    let radius = CELL_SIZE_M * 0.5;
    let extent = radius * normal.iter().map(|n| n.abs()).sum::<f64>();
    let mut lo = -extent;
    let mut hi = extent;
    for _ in 0..24 {
        let mid = (lo + hi) * 0.5;
        if volume_fraction(normal, mid, radius) > theta {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    let offset = (lo + hi) * 0.5;
    Some(Plane { normal, offset })
}

pub(crate) fn swept_fraction(plane: Plane, axis: usize, positive: bool, width: f64) -> f64 {
    let radius = CELL_SIZE_M * 0.5;
    let width = width.clamp(0.0, CELL_SIZE_M);
    if width <= 0.0 {
        return 0.0;
    }
    if width >= CELL_SIZE_M {
        return volume_fraction(plane.normal, plane.offset, radius);
    }
    let low = if positive { radius - width } else { -radius };
    let high = if positive { radius } else { -radius + width };
    let mut wet = 0;
    let mut total = 0;
    for i in 0..SAMPLES {
        for j in 0..SAMPLES {
            for k in 0..SAMPLES {
                let mut point = [0.0; 3];
                point[axis] = low + (k as f64 + 0.5) / SAMPLES as f64 * (high - low);
                point[(axis + 1) % 3] = ((i as f64 + 0.5) / SAMPLES as f64 - 0.5) * CELL_SIZE_M;
                point[(axis + 2) % 3] = ((j as f64 + 0.5) / SAMPLES as f64 - 0.5) * CELL_SIZE_M;
                if dot(plane.normal, point) <= plane.offset {
                    wet += 1;
                }
                total += 1;
            }
        }
    }
    wet as f64 / total as f64
}

fn volume_fraction(normal: [f64; 3], offset: f64, radius: f64) -> f64 {
    let mut wet = 0;
    for x in 0..SAMPLES {
        for y in 0..SAMPLES {
            for z in 0..SAMPLES {
                let p = [x, y, z].map(|v| ((v as f64 + 0.5) / SAMPLES as f64 - 0.5) * radius * 2.0);
                if dot(normal, p) <= offset {
                    wet += 1;
                }
            }
        }
    }
    wet as f64 / (SAMPLES * SAMPLES * SAMPLES) as f64
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
