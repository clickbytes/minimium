use super::{character_frame, CharacterGravity, PLAYER_EYE_HEIGHT, PLAYER_HEIGHT, PLAYER_RADIUS};

/// Stable tangent frame used by character movement, rays and presentation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterFrame {
    pub up: [f64; 3],
    pub right: [f64; 3],
    pub forward: [f64; 3],
}

/// Position, body dimensions, eye point and local frame for one character.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CharacterGeometry {
    feet_m: [f64; 3],
    frame: CharacterFrame,
}

impl CharacterGeometry {
    /// Builds the shared character geometry from world-space feet and gravity.
    /// Invalid positions or radial centers have no usable character frame. A
    /// character exactly at its radial center uses the existing +Y fallback;
    /// otherwise radial gravity uses the normalized world-to-center direction
    /// and the same stable tangent basis as `character_frame`.
    pub fn new(feet_m: [f64; 3], gravity: CharacterGravity) -> Option<Self> {
        if !feet_m
            .iter()
            .all(|value| value.is_finite() && value.abs() <= f64::from(f32::MAX))
        {
            return None;
        }
        let position = feet_m.map(|value| value as f32);
        if let CharacterGravity::Radial { center } = gravity {
            if !center.iter().all(|value| value.is_finite()) {
                return None;
            }
            let radial = std::array::from_fn::<_, 3, _>(|axis| position[axis] - center[axis]);
            if !radial.iter().all(|value| value.is_finite())
                || !radial
                    .iter()
                    .map(|value| value * value)
                    .sum::<f32>()
                    .is_finite()
            {
                return None;
            }
        }
        let (up, right, forward) = character_frame(gravity, position);
        let basis = [up, right, forward];
        if basis.iter().any(|axis| {
            !axis.iter().all(|value| value.is_finite())
                || (axis.iter().map(|value| value * value).sum::<f32>() - 1.0).abs() > 1.0e-4
        }) || dot_f32(up, right).abs() > 1.0e-4
            || dot_f32(up, forward).abs() > 1.0e-4
            || dot_f32(right, forward).abs() > 1.0e-4
        {
            return None;
        }
        Some(Self {
            feet_m,
            frame: CharacterFrame {
                up: up.map(f64::from),
                right: right.map(f64::from),
                forward: forward.map(f64::from),
            },
        })
    }

    pub fn feet_m(self) -> [f64; 3] {
        self.feet_m
    }

    pub fn frame(self) -> CharacterFrame {
        self.frame
    }

    pub const fn radius_m(self) -> f32 {
        PLAYER_RADIUS
    }

    pub const fn height_m(self) -> f32 {
        PLAYER_HEIGHT
    }

    pub const fn eye_height_m(self) -> f32 {
        PLAYER_EYE_HEIGHT
    }

    /// Sign of the local-to-world determinant for basis columns [right, up, forward].
    /// Negative values require reversed triangle winding for transformed meshes.
    pub fn frame_handedness(self) -> f64 {
        let right_cross_up = cross(self.frame.right, self.frame.up);
        dot(right_cross_up, self.frame.forward).signum()
    }

    pub fn body_size_m(self) -> [f32; 3] {
        [PLAYER_RADIUS * 2.0, PLAYER_HEIGHT, PLAYER_RADIUS * 2.0]
    }

    pub fn eye_position_m(self) -> [f64; 3] {
        self.local_to_world_point([0.0, f64::from(PLAYER_EYE_HEIGHT), 0.0])
    }

    pub fn body_center_m(self) -> [f64; 3] {
        self.local_to_world_point([0.0, f64::from(PLAYER_HEIGHT) * 0.5, 0.0])
    }

    /// Conservative sphere covering the oriented local body box.
    pub fn body_bounding_radius_m(self) -> f64 {
        let radius = f64::from(PLAYER_RADIUS);
        let half_height = f64::from(PLAYER_HEIGHT) * 0.5;
        (2.0 * radius * radius + half_height * half_height).sqrt()
    }

    pub fn world_to_local_point(self, point_m: [f64; 3]) -> [f64; 3] {
        let delta = std::array::from_fn(|axis| point_m[axis] - self.feet_m[axis]);
        [
            dot(delta, self.frame.right),
            dot(delta, self.frame.up),
            dot(delta, self.frame.forward),
        ]
    }

    pub fn world_to_local_direction(self, direction: [f64; 3]) -> [f64; 3] {
        [
            dot(direction, self.frame.right),
            dot(direction, self.frame.up),
            dot(direction, self.frame.forward),
        ]
    }

    pub fn local_to_world_point(self, point_m: [f64; 3]) -> [f64; 3] {
        std::array::from_fn(|axis| {
            self.feet_m[axis]
                + self.frame.right[axis] * point_m[0]
                + self.frame.up[axis] * point_m[1]
                + self.frame.forward[axis] * point_m[2]
        })
    }

    pub fn local_to_world_direction(self, direction: [f64; 3]) -> [f64; 3] {
        std::array::from_fn(|axis| {
            self.frame.right[axis] * direction[0]
                + self.frame.up[axis] * direction[1]
                + self.frame.forward[axis] * direction[2]
        })
    }
}

fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    (0..3).map(|axis| left[axis] * right[axis]).sum()
}

fn dot_f32(left: [f32; 3], right: [f32; 3]) -> f32 {
    (0..3).map(|axis| left[axis] * right[axis]).sum()
}

fn cross(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}
