use super::super::*;

pub(in crate::renderer) fn camera_direction(
    character: CharacterState,
    yaw: f32,
    pitch: f32,
) -> glam::Vec3 {
    let frame = CharacterGeometry::new(character.position.map(f64::from), character.gravity)
        .map(|geometry| geometry.frame())
        .unwrap_or_else(|| fallback_character_frame(character));
    camera_direction_from_frame(frame, yaw, pitch)
}

pub(super) fn fallback_character_frame(
    character: CharacterState,
) -> minimium_world::world::CharacterFrame {
    let (up, right, forward) = character_frame(character.gravity, character.position);
    minimium_world::world::CharacterFrame {
        up: up.map(f64::from),
        right: right.map(f64::from),
        forward: forward.map(f64::from),
    }
}

pub(in crate::renderer) fn camera_direction_from_geometry(
    geometry: CharacterGeometry,
    yaw: f32,
    pitch: f32,
) -> glam::Vec3 {
    camera_direction_from_frame(geometry.frame(), yaw, pitch)
}

fn camera_direction_from_frame(
    frame: minimium_world::world::CharacterFrame,
    yaw: f32,
    pitch: f32,
) -> glam::Vec3 {
    let up = glam::Vec3::from_array(frame.up.map(|value| value as f32));
    let right = glam::Vec3::from_array(frame.right.map(|value| value as f32));
    let forward = glam::Vec3::from_array(frame.forward.map(|value| value as f32));
    forward * (yaw.cos() * pitch.cos()) + right * (yaw.sin() * pitch.cos()) + up * pitch.sin()
}
