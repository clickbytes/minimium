use minimium_world::world::{
    CubeSampleKey, PlanetFace, WorldManifest, PLANET_SAMPLE_COORDINATE_SCALE,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum PolicyKind {
    Identity,
    CubeFaceRamp {
        face: PlanetFace,
        inner_distance: i32,
        outer_distance: i32,
    },
}

/// Neutral relief multiplier policy over integer cube-face sample coordinates.
/// Scenario-specific face and distances are selected by the composing game.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PlanetReliefSamplingPolicy(PolicyKind);

impl PlanetReliefSamplingPolicy {
    pub const fn identity() -> Self {
        Self(PolicyKind::Identity)
    }

    pub fn cube_face_ramp(
        face: PlanetFace,
        inner_distance: i32,
        outer_distance: i32,
    ) -> Option<Self> {
        if inner_distance < 0
            || inner_distance >= outer_distance
            || outer_distance > PLANET_SAMPLE_COORDINATE_SCALE
        {
            return None;
        }
        Some(Self(PolicyKind::CubeFaceRamp {
            face,
            inner_distance,
            outer_distance,
        }))
    }

    pub(crate) fn attenuation_permille(self, sample: CubeSampleKey) -> i64 {
        let PolicyKind::CubeFaceRamp {
            face,
            inner_distance,
            outer_distance,
        } = self.0
        else {
            return 1000;
        };
        let coordinates = sample.coordinates();
        let (normal_axis, normal_sign, tangent_axes) = face_axes(face);
        if coordinates[normal_axis] != normal_sign * PLANET_SAMPLE_COORDINATE_SCALE {
            return 1000;
        }
        let distance = coordinates[tangent_axes[0]]
            .abs()
            .max(coordinates[tangent_axes[1]].abs());
        let span = outer_distance - inner_distance;
        i64::from((distance - inner_distance).clamp(0, span)) * 1000 / i64::from(span)
    }
}

/// Immutable sample-selected relief policy bound to resolved world content.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ResolvedPlanetReliefPolicy {
    policy: PlanetReliefSamplingPolicy,
    manifest: WorldManifest,
    map_fingerprint: u64,
}

impl ResolvedPlanetReliefPolicy {
    pub const fn bind(
        policy: PlanetReliefSamplingPolicy,
        manifest: WorldManifest,
        map_fingerprint: u64,
    ) -> Self {
        Self {
            policy,
            manifest,
            map_fingerprint,
        }
    }

    pub const fn policy(self) -> PlanetReliefSamplingPolicy {
        self.policy
    }

    pub fn matches(self, manifest: WorldManifest, map_fingerprint: u64) -> bool {
        self.manifest == manifest && self.map_fingerprint == map_fingerprint
    }

    pub fn matches_manifest(self, manifest: WorldManifest) -> bool {
        self.manifest == manifest
    }
}

fn face_axes(face: PlanetFace) -> (usize, i32, [usize; 2]) {
    match face {
        PlanetFace::PositiveX => (0, 1, [1, 2]),
        PlanetFace::NegativeX => (0, -1, [1, 2]),
        PlanetFace::PositiveY => (1, 1, [0, 2]),
        PlanetFace::NegativeY => (1, -1, [0, 2]),
        PlanetFace::PositiveZ => (2, 1, [0, 1]),
        PlanetFace::NegativeZ => (2, -1, [0, 1]),
    }
}
