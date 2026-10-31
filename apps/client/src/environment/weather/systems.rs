//! Organized, reconstructible forcings; wind never calls this module.
use super::{
    clouds::{coordinates, types::random},
    dynamics, transport, wind,
};
use glam::DVec3;

pub(super) fn forcing(lat: f64, lon: f64, days: f64, seed: u128) -> [f64; 3] {
    let up = DVec3::new(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin());
    let frame = transport::frame(days * 86400.0, seed);
    let mut wet = 0.0;
    let mut frontal = 0.0;
    let mut dry = 0.0;
    let mut convective = 0.0;
    for cohort in 0..3 {
        let origin = frame.origin(up, 5000.0, cohort);
        let age = frame.cohorts[cohort].age / transport::COHORT_SECONDS;
        let life = wind::smooth(0.0, 0.8, age) * (1.0 - wind::smooth(1.8, 3.0, age));
        for cell in coordinates::cap(origin, 0.5, 2) {
            let key = seed as u64
                ^ (seed >> 64) as u64
                ^ (frame.cohorts[cohort].id as u64).wrapping_mul(719)
                ^ (u64::from(cell.face) << 24)
                ^ (u64::from(cell.x) << 12)
                ^ u64::from(cell.y);
            let center = cell.direction(random(key, 1), random(key, 2));
            let flow = frame.wind.velocity(center, 5000.0);
            // Meridional humidity/temperature gradients turn the band across flow.
            let gradient = wind::tangent(DVec3::Y, center) * (center.y * 0.35);
            let axis = (flow.normalize_or_zero() + gradient).normalize_or_zero();
            let side = axis.cross(center);
            let delta = origin - center;
            let along = delta.dot(axis);
            let across = delta.dot(side);
            let uneven = dynamics::noise((origin * 31.0).to_array(), key as u32) - 0.5;
            let strength = life * (0.5 + random(key, 3) * 0.5);
            let broad = (-(delta.length() / 0.28).powi(2)).exp() * strength;
            let band = (-((across + uneven * 0.025) / 0.045).powi(2) - (along / 0.38).powi(2))
                .exp()
                * strength;
            match (random(key, 4) * 4.0) as u8 {
                0 => {
                    frontal += band;
                    wet += broad * 0.45;
                }
                1 => wet += broad,
                2 => dry += broad,
                _ => convective += broad * (0.6 + uneven * 0.7),
            }
        }
    }
    [
        (0.42 + wet * 0.48 + frontal * 0.3 - dry * 0.7).clamp(0.0, 1.0),
        (0.35 + frontal * 0.7 + convective * 0.22 - dry * 0.25).clamp(0.0, 1.0),
        (0.35 + wet * 0.25 + frontal * 0.4 + convective * 0.3 - dry * 0.4).clamp(0.0, 1.0),
    ]
}
