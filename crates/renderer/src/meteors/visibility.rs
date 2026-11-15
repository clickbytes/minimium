//! CPU visibility policy; cloud-face ordering remains in the shared GPU mask.

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
