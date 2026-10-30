/// Local rendering profile. Quality never changes the astronomical inputs.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum SkyQuality {
    Low,
    #[default]
    Standard,
}

impl SkyQuality {
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "low" => Ok(Self::Low),
            "standard" => Ok(Self::Standard),
            _ => Err("sky quality must be low or standard".to_owned()),
        }
    }

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Standard => "standard",
        }
    }

    pub(crate) const fn profile(self) -> &'static SkyQualityProfile {
        match self {
            Self::Low => &SKY_QUALITY_PROFILES[0],
            Self::Standard => &SKY_QUALITY_PROFILES[1],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SkyQualityProfile {
    pub(crate) quality: SkyQuality,
    pub(crate) shadows_enabled: bool,
    pub(crate) shadow_resolution: u32,
    pub(crate) shadow_receiver_radius_m: f32,
    pub(crate) shadow_caster_radius_m: f32,
    pub(crate) shadow_fade_start_m: f32,
    pub(crate) shadow_constant_bias_m: f32,
    pub(crate) shadow_slope_bias_m: f32,
    pub(crate) shadow_max_bias_m: f32,
}

pub(crate) const SKY_QUALITY_PROFILES: [SkyQualityProfile; 2] = [
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
pub(crate) struct SkyQualityState {
    pub(crate) requested: SkyQuality,
    pub(crate) effective: SkyQuality,
    pub(crate) fallback_reason: Option<&'static str>,
}

impl SkyQualityState {
    pub(crate) const fn new(
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
