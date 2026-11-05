//! Visual shadow quality settings and their fixed resolution/strength parameters.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowSettings {
    pub enabled: bool,
    pub resolution: u32,
    pub receiver_radius_m: f32,
    pub caster_radius_m: f32,
    pub fade_start_m: f32,
    pub constant_bias_m: f32,
    pub slope_bias_m: f32,
    pub max_bias_m: f32,
}

impl ShadowSettings {
    pub fn for_quality(quality: crate::quality::SkyQuality) -> Self {
        let profile = quality.profile();
        Self {
            enabled: profile.shadows_enabled,
            resolution: profile.shadow_resolution,
            receiver_radius_m: profile.shadow_receiver_radius_m,
            caster_radius_m: profile.shadow_caster_radius_m,
            fade_start_m: profile.shadow_fade_start_m,
            constant_bias_m: profile.shadow_constant_bias_m,
            slope_bias_m: profile.shadow_slope_bias_m,
            max_bias_m: profile.shadow_max_bias_m,
        }
    }
}
