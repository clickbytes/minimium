use super::WeatherRuntime;
use minimium_core::world::CollisionIndex;
use std::time::{Duration, Instant};

pub(super) fn mask(
    runtime: &mut WeatherRuntime,
    index: &CollisionIndex,
    eye: [f32; 3],
    up: [f32; 3],
) -> [u16; 4] {
    let moved = eye
        .iter()
        .zip(runtime.shelter_eye)
        .any(|(current, previous)| (current - previous).abs() > 1.5);
    if !moved
        && runtime
            .shelter_updated
            .is_some_and(|time| time.elapsed() < Duration::from_millis(300))
    {
        return runtime.shelter_mask;
    }
    let up = glam::Vec3::from_array(up).normalize_or_zero();
    let reference = if up.y.abs() > 0.95 {
        glam::Vec3::X
    } else {
        glam::Vec3::Y
    };
    let east = up.cross(reference).normalize_or_zero();
    let north = east.cross(up).normalize_or_zero();
    let eye_vector = glam::Vec3::from_array(eye);
    let mut mask = [0u16; 4];
    for row in 0..8 {
        for column in 0..8 {
            let offset_east = (column as f32 * 4.0 - 14.0) * 1.4;
            let offset_north = (row as f32 * 4.0 - 14.0) * 1.4;
            let point = eye_vector + east * offset_east + north * offset_north;
            if runtime
                .cover_query
                .covered(index, point.to_array(), up.to_array(), 20.0)
            {
                let cell = row * 8 + column;
                mask[cell / 16] |= 1 << (cell % 16);
            }
        }
    }
    runtime.shelter_mask = mask;
    runtime.shelter_eye = eye;
    runtime.shelter_updated = Some(Instant::now());
    mask
}
