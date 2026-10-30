use super::{CharacterGravity, WorldManifest, WorldTopology};

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
