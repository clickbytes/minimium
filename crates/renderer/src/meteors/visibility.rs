//! CPU visibility policy; cloud-face ordering remains in the shared GPU mask.
use glam::DVec3;

#[derive(Clone, Copy, Debug, Default)]
pub enum VisibilityPolicy {
    /// Low quality supports only sources above the cloud decks.
    HighAtmosphereOnly,
    /// Surface depth plus finite source-distance cloud-face transmission.
    #[default]
    FiniteDistance,
}

impl VisibilityPolicy {
    pub fn from_quality(quality: crate::quality::SkyQuality) -> Self {
        match quality {
            crate::quality::SkyQuality::Low => Self::HighAtmosphereOnly,
            crate::quality::SkyQuality::Standard => Self::FiniteDistance,
        }
    }

    pub const fn minimum_height_m(self) -> f64 {
        match self {
            Self::HighAtmosphereOnly => 20_000.0,
            Self::FiniteDistance => 0.0,
        }
    }
}

/// Conservative head-only rejection. A hidden head never rejects its trail:
/// individual segments retain the shader's exact terrain/planet ray check.
pub fn head_visible(eye: [f64; 3], source: [f64; 3], radius_m: f64, extent_m: f64) -> bool {
    let eye = DVec3::from_array(eye);
    let source = DVec3::from_array(source);
    if !eye.is_finite() || !source.is_finite() || !radius_m.is_finite() || radius_m <= 0.0 {
        return false;
    }
    let delta = source - eye;
    let length = delta.length();
    if length <= extent_m.max(0.0) {
        return true;
    }
    let ray = delta / length;
    let distance = (-eye.dot(ray)).clamp(0.0, (length - extent_m.max(0.0)).max(0.0));
    let closest = (eye + ray * distance).length();
    // Expanding the head can only keep it, never hide a visible edge.
    closest + extent_m.max(0.0) >= radius_m - (radius_m * 1e-9).max(0.01)
}
