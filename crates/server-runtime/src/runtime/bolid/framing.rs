//! Bounded manual placement inside the smallest supported camera field of view.
use super::{add, atmospheric_intersection, cross, dot, length_squared, normalize, scale, sub};

pub(super) fn in_view(
    observer: [f32; 3],
    gaze: [f32; 3],
    radius: f32,
    length_scale: f32,
    seed: u64,
    kind: u8,
) -> Option<([f32; 3], [f32; 3])> {
    let gaze = normalize(gaze);
    let reference = if gaze[2].abs() < 0.9 {
        [0.0, 0.0, 1.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let right = normalize(cross(gaze, reference));
    let up = cross(right, gaze);
    let phase = seed as u32 as f32 / u32::MAX as f32 * std::f32::consts::TAU;
    let mut starts = Vec::with_capacity(49);
    let mut ends = Vec::with_capacity(49);
    let heights = if kind == 2 {
        [108_000.0, 37_000.0]
    } else {
        [101_000.0, 48_000.0]
    };
    // A 12-degree cone leaves three degrees of margin in the minimum 30-degree
    // vertical FOV. A straight segment between its points stays in that cone.
    for index in 0..49 {
        let ring = if index == 0 { 0 } else { 1 + (index - 1) / 16 };
        let angle = (ring as f32 * 4.0).to_radians();
        let azimuth = phase + index as f32 * std::f32::consts::TAU / 16.0;
        let transverse = add(scale(right, azimuth.cos()), scale(up, azimuth.sin()));
        let ray = normalize(add(
            scale(gaze, angle.cos()),
            scale(transverse, angle.sin()),
        ));
        for (height, points) in [(heights[0], &mut starts), (heights[1], &mut ends)] {
            if let Some(distance) =
                atmospheric_intersection(observer, ray, radius + height * length_scale)
            {
                if distance > 1000.0 * length_scale {
                    points.push((add(observer, scale(ray, distance)), ray));
                }
            }
        }
    }
    let mut best = None;
    let mut best_score = f32::INFINITY;
    for &(start, start_ray) in &starts {
        for &(end, end_ray) in &ends {
            let delta = sub(end, start);
            let physical_length = length_squared(delta).sqrt() / length_scale;
            // Radial velocity increases along a straight chord. A negative
            // endpoint value therefore proves descent throughout the segment.
            if dot(end, delta) >= 0.0 || !(20_000.0..=300_000.0).contains(&physical_length) {
                continue;
            }
            let score = (dot(start_ray, end_ray) - 16.0_f32.to_radians().cos()).abs() * 10.0
                + (2.0 - dot(start_ray, gaze) - dot(end_ray, gaze)) * 0.2
                + physical_length / 1_000_000.0;
            if score < best_score {
                best_score = score;
                best = Some((start, end));
            }
        }
    }
    best
}
