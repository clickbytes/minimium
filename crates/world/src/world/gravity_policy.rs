use super::{WorldManifest, WorldTopology};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum CharacterGravity {
    #[default]
    FixedY,
    Radial {
        center: [f32; 3],
    },
}

/// Returns the local up direction and a stable tangent basis for a character.
pub fn character_frame(
    gravity: CharacterGravity,
    position: [f32; 3],
) -> ([f32; 3], [f32; 3], [f32; 3]) {
    let up = match gravity {
        CharacterGravity::FixedY => [0.0, 1.0, 0.0],
        CharacterGravity::Radial { center } => {
            normalize(subtract(position, center)).unwrap_or([0.0, 1.0, 0.0])
        }
    };
    let reference = if up[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let projected = subtract(reference, scale(up, dot(reference, up)));
    let forward = normalize(projected).unwrap_or([0.0, 0.0, -1.0]);
    let right = normalize(cross(forward, up)).unwrap_or([1.0, 0.0, 0.0]);
    (up, right, forward)
}

fn dot(left: [f32; 3], right: [f32; 3]) -> f32 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn cross(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

fn subtract(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn scale(vector: [f32; 3], amount: f32) -> [f32; 3] {
    [vector[0] * amount, vector[1] * amount, vector[2] * amount]
}

fn normalize(vector: [f32; 3]) -> Option<[f32; 3]> {
    let length = dot(vector, vector).sqrt();
    (length > 1.0e-8 && length.is_finite()).then(|| scale(vector, length.recip()))
}

/// Selects the character gravity frame for a world without defining a solver.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GravityPolicy {
    FixedY,
    Radial { center_m: [f64; 3] },
}

impl GravityPolicy {
    /// Creates a radial policy when its center can be represented by the simulation.
    pub fn radial(center_m: [f64; 3]) -> Option<Self> {
        center_m
            .iter()
            .all(|value| value.is_finite() && value.abs() <= f64::from(f32::MAX))
            .then_some(Self::Radial { center_m })
    }

    /// Preserves the existing topology-based Earth/procedural and dungeon behavior.
    pub fn for_manifest(manifest: WorldManifest) -> Self {
        match manifest.topology() {
            WorldTopology::Planet => Self::Radial { center_m: [0.0; 3] },
            WorldTopology::Dungeon => Self::FixedY,
        }
    }

    pub fn character_gravity(self) -> CharacterGravity {
        match self {
            Self::FixedY => CharacterGravity::FixedY,
            Self::Radial { center_m } => CharacterGravity::Radial {
                center: center_m.map(|value| value as f32),
            },
        }
    }
}
