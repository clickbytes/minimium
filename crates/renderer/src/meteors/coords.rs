//! The single physical ↔ game ↔ render-origin mapping adapter for local meteors.
//!
//! ADR 0013 § 4 conventions, owned nowhere else:
//! - input physics: positions and velocities in physical metres as `f64` on
//!   world-aligned axes centred on the planet;
//! - game world: world metres in `f64` (`RenderViewFrame` scale); the vertical
//!   mapping reuses the planet manifest (`radius_mm`, `height_scale_permille`),
//!   never a hard-coded radius;
//! - render: `f32` metres relative to the current render origin; large absolute
//!   `f32` differences are never formed — the `f64` anchors are subtracted first,
//!   exactly like the terrain eye-relative uniform.
//!
//! The scale used here is the same one the observer path applies through
//! `camera_reference` and `prepare_bolids` applies to event lengths; no second
//! convention may be introduced for meteors.

use minimium_environment::sky::EarthHeightReference;

/// Vertical mapping between physical altitude and radial game metres.
///
/// Height scale is supplied by the world manifest (`height_scale_permille`,
/// always validated against `1..=1000`), never a constant. Surface anchors
/// come from the GEBCO reference ("Earth", relief enabled) or stay zero
/// (uniform scaling).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlanetSurfaceScale {
    /// Game metres along the radial axis per one physical metre.
    height_scale: f64,
    /// Radial game offset (above the game planet radius) where the anchor applies.
    baseline_game_m: f64,
    /// Physical altitude represented at the baseline.
    physical_anchor_m: f64,
}

impl PlanetSurfaceScale {
    /// The neutral unit mapping (permille 1000, zero surface anchors).
    pub const UNIT: Self = Self::new(1000, 0.0, 0.0);

    /// Uniform mapping without surface anchors (procedural and relief-free).
    /// `height_scale_permille` must be validated in `1..=1000` before calling.
    pub const fn from_permille(height_scale_permille: u32) -> Option<Self> {
        if height_scale_permille < 1 || height_scale_permille > 1000 {
            return None;
        }
        Some(Self::new(height_scale_permille, 0.0, 0.0))
    }

    /// Earth relief mapping anchored on the sampled surface reference.
    /// `height_scale_permille` must be validated in `1..=1000` before calling.
    pub const fn from_earth(
        height_scale_permille: u32,
        reference: EarthHeightReference,
    ) -> Option<Self> {
        if height_scale_permille < 1 || height_scale_permille > 1000 {
            return None;
        }
        Some(Self::new(
            height_scale_permille,
            reference.baseline_game_mm as f64 / 1000.0,
            reference.physical_anchor_m,
        ))
    }

    const fn new(height_scale_permille: u32, baseline_game_m: f64, physical_anchor_m: f64) -> Self {
        Self {
            height_scale: height_scale_permille as f64 / 1000.0,
            baseline_game_m,
            physical_anchor_m,
        }
    }

    /// Radial game offset (metres above the game planet radius) at a physical
    /// altitude; the exact inverse of [`Self::physical_altitude_m`].
    pub fn radial_offset_m(self, physical_altitude_m: f64) -> f64 {
        self.baseline_game_m + (physical_altitude_m - self.physical_anchor_m) * self.height_scale
    }

    /// Physical altitude implied by a radial game offset above the game planet
    /// radius; mirrors the reference camera mapping without its astronomy cap.
    pub fn physical_altitude_m(self, radial_offset_game_m: f64) -> f64 {
        self.physical_anchor_m + (radial_offset_game_m - self.baseline_game_m) / self.height_scale
    }
}

/// Converts a world-anchored `f64` position to `f32` metres relative to the
/// current render origin. The subtraction happens in `f64`; the result must
/// come from a source that already culled far-away positions.
pub fn render_relative_m(world_m: [f64; 3], render_origin_m: [f64; 3]) -> [f32; 3] {
    [
        (world_m[0] - render_origin_m[0]) as f32,
        (world_m[1] - render_origin_m[1]) as f32,
        (world_m[2] - render_origin_m[2]) as f32,
    ]
}
