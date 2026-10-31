//! Shared polynomial kernel: rho = coefficient * max(1 - dot(q,q), 0)^2.
//! Integrals are centered at closest approach, avoiding planetary cancellation.
use super::CloudLobe;
use glam::DVec3;

pub(crate) fn point(lobe: &CloudLobe, position: DVec3) -> f64 {
    if position.dot(lobe.base_normal) < lobe.base_offset {
        return 0.0;
    }
    let q = (lobe.orientation.conjugate() * (position - lobe.center)) / lobe.axes;
    lobe.density * (1.0 - q.length_squared()).max(0.0).powi(2)
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
    let a = d.length_squared();
    let t = -q.dot(d) / a;
    let h = (1.0 - (q + d * t).length_squared()).max(0.0);
    let primitive = |u: f64| h * h * u - 2.0 * h * a * u.powi(3) / 3.0 + a * a * u.powi(5) / 5.0;
    (lobe.density * (primitive(hi - t) - primitive(lo - t))).max(0.0)
}

/// Exact clipped kernel volume, including an arbitrarily tilted ellipsoid/base.
pub(crate) fn volume(lobe: &CloudLobe) -> f64 {
    let n = (lobe.orientation.conjugate() * lobe.base_normal) * lobe.axes;
    let z = ((lobe.base_offset - lobe.center.dot(lobe.base_normal)) / n.length()).clamp(-1.0, 1.0);
    let p = |z: f64| z - z.powi(3) + 0.6 * z.powi(5) - z.powi(7) / 7.0;
    std::f64::consts::PI / 3.0 * (p(1.0) - p(z)) * lobe.axes.element_product()
}
