//! Canonical physical cloud density, mirrored by the WGSL near-cloud shader.
//! Bounded lobes retain their IDs, motion, mass and clipping planes.
use super::{detail, CloudLobe};
use glam::DVec3;

pub(crate) fn point(lobe: &CloudLobe, position: DVec3) -> f64 {
    if position.dot(lobe.base_normal) < lobe.base_offset {
        return 0.0;
    }
    let q = (lobe.orientation.conjugate() * (position - lobe.center)) / lobe.axes;
    detail::kernel(q, (lobe.id.hash() >> 40) as u32) * lobe.density
}

pub(crate) fn interval(lobe: &CloudLobe, origin: DVec3, ray: DVec3) -> Option<(f64, f64)> {
    let inverse = lobe.orientation.conjugate();
    let q = (inverse * (origin - lobe.center)) / lobe.axes;
    let d = (inverse * ray) / lobe.axes;
    let speed2 = d.length_squared();
    let closest = -q.dot(d) / speed2;
    let h = 1.0 - (q + d * closest).length_squared();
    if h <= 0.0 {
        return None;
    }
    let half = (h / speed2).sqrt();
    let mut lo = (closest - half).max(0.0);
    let mut hi = closest + half;
    let slope = ray.dot(lobe.base_normal);
    let offset = lobe.base_offset - origin.dot(lobe.base_normal);
    if slope.abs() < 1e-7 {
        if offset > 0.0 {
            return None;
        }
    } else if slope > 0.0 {
        lo = lo.max(offset / slope);
    } else {
        hi = hi.min(offset / slope);
    }
    (hi > lo).then_some((lo, hi))
}

pub(crate) fn integral(lobe: &CloudLobe, origin: DVec3, ray: DVec3, lo: f64, hi: f64) -> f64 {
    let Some((entry, exit)) = interval(lobe, origin, ray) else {
        return 0.0;
    };
    let (lo, hi) = (lo.max(entry), hi.min(exit));
    if hi <= lo {
        return 0.0;
    }
    let inverse = lobe.orientation.conjugate();
    let q = (inverse * (origin - lobe.center)) / lobe.axes;
    let d = (inverse * ray) / lobe.axes;
    // Six-point Gauss-Legendre quadrature resolves eroded cloud boundaries.
    // The camera and globe both integrate this same canonical density.
    const NODES: [f64; 6] = [
        0.033765242898, 0.169395306767, 0.380690406958,
        0.619309593042, 0.830604693233, 0.966234757102,
    ];
    const WEIGHTS: [f64; 6] = [
        0.085662246190, 0.180380786524, 0.233956967286,
        0.233956967286, 0.180380786524, 0.085662246190,
    ];
    let mut sum = 0.0;
    let length = hi - lo;
    let seed = (lobe.id.hash() >> 40) as u32;
    for (node, weight) in NODES.into_iter().zip(WEIGHTS) {
        sum += detail::kernel(q + d * (lo + node * length), seed) * weight;
    }
    (lobe.density * length * sum).max(0.0)
}

/// Exact base polynomial volume; detailed erosion is an approximate visual
/// redistribution of extinction, not an additional mass source.
pub(crate) fn volume(lobe: &CloudLobe) -> f64 {
    let n = (lobe.orientation.conjugate() * lobe.base_normal) * lobe.axes;
    let z = ((lobe.base_offset - lobe.center.dot(lobe.base_normal)) / n.length()).clamp(-1.0, 1.0);
    let p = |z: f64| z - z.powi(3) + 0.6 * z.powi(5) - z.powi(7) / 7.0;
    std::f64::consts::PI / 3.0 * (p(1.0) - p(z)) * lobe.axes.element_product()
}
