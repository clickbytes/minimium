//! CPU selection of the same finite point source evaluated by sky_common.wgsl.
use crate::AtmosphereUniform;
use glam::DVec3;

/// Analytic flash metering in the same scene units as incident_luminance.
/// Weak sources leave exposure unchanged. Strong flashes limit the exposed
/// reference illumination smoothly, without modifying source energy or range.
/// This is deterministic frame metering, not temporal eye adaptation.
pub fn exposure_scale(incident: f64, exposure: f32) -> f32 {
    let exposed = incident * f64::from(exposure);
    if !exposed.is_finite() || exposed <= 1.0 {
        return 1.0;
    }
    let excess = exposed - 1.0;
    let shoulder = excess * (excess / (1.0 + excess));
    (1.0 / (1.0 + 0.36 * shoulder)) as f32
}

/// Unexposed luminance at a receiver, before its surface cosine/BRDF.
/// Keep the eight-sample quadrature in sync with bolid_air_transmission WGSL.
pub fn incident_luminance(
    position_power: [f32; 4],
    color: [f32; 4],
    receiver: [f64; 3],
    atmosphere: &AtmosphereUniform,
) -> f64 {
    let point = DVec3::from_array(receiver);
    let emitter = DVec3::new(
        position_power[0] as f64,
        position_power[1] as f64,
        position_power[2] as f64,
    );
    let delta = emitter - point;
    let distance = delta.length();
    if position_power[3] <= 0.0 || !distance.is_finite() || distance < 1e-6 {
        return 0.0;
    }
    let ray = delta / distance;
    let radius = f64::from(atmosphere.config[0]);
    let b = point.dot(ray);
    let c = (point.length() - radius) * (point.length() + radius);
    let discriminant = b * b - c;
    if radius > 0.0 && discriminant >= 0.0 {
        let hit = -b - discriminant.sqrt();
        if (hit > 1.0 && hit < distance) || (b < 0.0 && c <= 0.0) {
            return 0.0;
        }
    }
    let mut optical_depth = DVec3::ZERO;
    let scale = f64::from(atmosphere.config[1]);
    if radius > 0.0 && scale > 0.0 {
        let optical_radius = f64::from(atmosphere.radii[0]);
        let height = (point.length() - radius) * scale;
        let start = point.normalize_or_zero() * (optical_radius + height);
        let length = distance * scale;
        let b = start.dot(ray);
        let c = height * (2.0 * optical_radius + height);
        let top = f64::from(atmosphere.radii[1]);
        let c_top = (height - top) * (2.0 * optical_radius + height + top);
        let root = (b * b - c_top).max(0.0).sqrt();
        let begin = (-b - root).max(0.0);
        let end = (-b + root).min(length).max(begin);
        // A miss has zero interval; all quadrature weights then vanish.
        let closest = (-b).clamp(begin, end);
        let left = if closest <= begin {
            0
        } else if closest >= end {
            8
        } else {
            4
        };
        for i in 0..8 {
            let (a, z) = if i < left {
                let u = 1.0 - i as f64 / left as f64;
                let v = 1.0 - (i + 1) as f64 / left as f64;
                (
                    closest - (closest - begin) * u.powi(3),
                    closest - (closest - begin) * v.powi(3),
                )
            } else {
                let u = (i - left) as f64 / (8 - left) as f64;
                let v = (i + 1 - left) as f64 / (8 - left) as f64;
                (
                    closest + (end - closest) * u.powi(3),
                    closest + (end - closest) * v.powi(3),
                )
            };
            let d = (a + z) * 0.5;
            let difference = c + 2.0 * b * d + d * d;
            let h = (difference
                / ((optical_radius * optical_radius + difference)
                    .max(1.0)
                    .sqrt()
                    + optical_radius))
                .max(0.0);
            if h < f64::from(atmosphere.radii[1]) {
                let rayleigh = (-h / f64::from(atmosphere.radii[2]).max(1.0)).exp();
                let aerosol = f64::from(atmosphere.mie[0])
                    * (-h / f64::from(atmosphere.radii[3]).max(1.0)).exp();
                let ozone = (1.0
                    - (h - f64::from(atmosphere.ozone[3])).abs()
                        / f64::from(atmosphere.mie[2]).max(1.0))
                .max(0.0);
                for channel in 0..3 {
                    optical_depth[channel] += (f64::from(atmosphere.rayleigh[channel]) * rayleigh
                        + aerosol
                        + f64::from(atmosphere.ozone[channel]) * ozone)
                        * (z - a);
                }
            }
        }
    }
    let weights = [0.2126, 0.7152, 0.0722];
    let luminance: f64 = (0..3)
        .map(|i| f64::from(color[i]) * (-optical_depth[i]).exp() * weights[i])
        .sum();
    luminance * f64::from(position_power[3]) * 1e10 / distance.powi(2).max(10000.0)
}
