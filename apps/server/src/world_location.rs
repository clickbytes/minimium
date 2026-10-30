use minimium_core::world::{
    CharacterGravity, Position3Mm, WorldInstanceId, WorldLocation, WorldOrientationQ30,
};

const RADIAL_SAVE_CLEARANCE: f32 = 0.01;

pub(crate) fn world_location(
    instance: WorldInstanceId,
    position: [f32; 3],
    gravity: CharacterGravity,
) -> WorldLocation {
    let up = minimium_core::world::character_frame(gravity, position).0;
    // A radial location sits on a curved triangle; millimeter rounding can
    // otherwise move its capsule a few millimeters back into the surface.
    let safe_position = if matches!(gravity, CharacterGravity::Radial { .. }) {
        std::array::from_fn(|axis| position[axis] + up[axis] * RADIAL_SAVE_CLEARANCE)
    } else {
        position
    };
    let quantized = safe_position.map(|value| (f64::from(value) * 1000.0).round() as i64);
    WorldLocation::new(
        instance,
        Position3Mm(quantized),
        orientation_for_gravity(gravity, safe_position),
    )
}

fn orientation_for_gravity(gravity: CharacterGravity, position: [f32; 3]) -> WorldOrientationQ30 {
    let up = minimium_core::world::character_frame(gravity, position).0;
    let denominator = (2.0 * (1.0 + up[1])).sqrt();
    let quaternion = if denominator <= 1.0e-6 {
        [0.0, 0.0, 0.0, 1.0]
    } else {
        [
            ((1.0 + up[1]) * 0.5).sqrt(),
            up[2] / denominator,
            0.0,
            -up[0] / denominator,
        ]
    };
    let fixed = quaternion.map(|value| (f64::from(value) * (1_i64 << 30) as f64).round() as i32);
    WorldOrientationQ30::new(fixed).unwrap_or(WorldOrientationQ30::IDENTITY)
}
