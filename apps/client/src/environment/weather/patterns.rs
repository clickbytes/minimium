//! Advected variation used by atmospheric columns, never cloud geometry.
use super::{
    dynamics,
    transport::{self, FlowFrame},
    wind,
};
use glam::DVec3;

pub(super) fn cohort_weight(frame: &FlowFrame, origin: DVec3, index: usize) -> f64 {
    let cohort = &frame.cohorts[index];
    let key = (frame.seed as u64 ^ (frame.seed >> 64) as u64) as u32 ^ cohort.id as u32;
    let location = dynamics::noise((origin * 48.0).to_array(), key);
    let age = cohort.age / transport::COHORT_SECONDS;
    let birth = location * 0.45;
    wind::smooth(birth, birth + 0.75, age) * (1.0 - wind::smooth(1.75 + location * 0.35, 3.0, age))
}

pub(super) fn weather_pattern(
    frame: &FlowFrame,
    up: DVec3,
    height: f64,
    scale: f64,
    salt: u32,
) -> f64 {
    let key = (frame.seed as u64 ^ (frame.seed >> 64) as u64) as u32;
    let mut sum = 0.0;
    let mut variance = 0.0;
    for index in 0..3 {
        let origin = frame.origin(up, height, index);
        let weight = cohort_weight(frame, origin, index);
        let seed = key ^ (frame.cohorts[index].id as u32).wrapping_mul(0x9e3779b9) ^ salt;
        sum += (dynamics::noise((origin * scale).to_array(), seed) - 0.5) * weight;
        variance += weight * weight;
    }
    (0.5 + sum / variance.sqrt().max(0.001)).clamp(0.0, 1.0)
}
