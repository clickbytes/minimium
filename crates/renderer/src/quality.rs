//! Visual quality profiles for renderer features.\n\n/// Local rendering profile. Quality never changes the astronomical inputs.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SkyQuality {
    Low,
    #[default]
    Standard,
}

impl SkyQuality {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "low" => Ok(Self::Low),
            "standard" => Ok(Self::Standard),
            _ => Err("sky quality must be low or standard".to_owned()),
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Standard => "standard",
        }
    }

    pub const fn profile(self) -> &'static SkyQualityProfile {
        match self {
            Self::Low => &SKY_QUALITY_PROFILES[0],
            Self::Standard => &SKY_QUALITY_PROFILES[1],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyQualityProfile {
    pub quality: SkyQuality,
    pub shadows_enabled: bool,
    pub shadow_resolution: u32,
    pub shadow_receiver_radius_m: f32,
    pub shadow_caster_radius_m: f32,
    pub shadow_fade_start_m: f32,
    pub shadow_constant_bias_m: f32,
    pub shadow_slope_bias_m: f32,
    pub shadow_max_bias_m: f32,
}

pub const SKY_QUALITY_PROFILES: [SkyQualityProfile; 2] = [
    SkyQualityProfile {
        quality: SkyQuality::Low,
        shadows_enabled: false,
        shadow_resolution: 1,
        shadow_receiver_radius_m: 128.0,
        shadow_caster_radius_m: 256.0,
        shadow_fade_start_m: 112.0,
        shadow_constant_bias_m: 0.02,
        shadow_slope_bias_m: 0.04,
        shadow_max_bias_m: 0.10,
    },
    SkyQualityProfile {
        quality: SkyQuality::Standard,
        shadows_enabled: true,
        shadow_resolution: 1024,
        shadow_receiver_radius_m: 128.0,
        shadow_caster_radius_m: 256.0,
        shadow_fade_start_m: 112.0,
        shadow_constant_bias_m: 0.02,
        shadow_slope_bias_m: 0.04,
        shadow_max_bias_m: 0.10,
    },
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkyQualityState {
    pub requested: SkyQuality,
    pub effective: SkyQuality,
    pub fallback_reason: Option<&'static str>,
}

impl SkyQualityState {
    pub const fn new(
        requested: SkyQuality,
        effective: SkyQuality,
        fallback_reason: Option<&'static str>,
    ) -> Self {
        Self {
            requested,
            effective,
            fallback_reason,
        }
    }
}
