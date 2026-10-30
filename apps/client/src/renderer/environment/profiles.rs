const MAX_PROFILE_RGB: f32 = 10_000.0;
const MAX_PROFILE_EXPOSURE: f32 = 100.0;
const MAX_LIGHT_STRENGTH: f32 = 1_000.0;
const MAX_PROFILE_THRESHOLD: f64 = 10_000.0;

/// Presentation controls expressed in the renderer's relative linear RGB scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LightingProfile {
    pub(crate) sunlight: SunlightProfile,
    pub(crate) moonlight: MoonlightProfile,
    pub(crate) ambient: [f32; 3],
    pub(crate) exposure: f32,
    pub(crate) halo_thresholds: HaloThresholds,
    pub(crate) halos: HaloShapeProfile,
    pub(crate) clear_sky: ClearSkyProfile,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ClearSkyProfile {
    pub(crate) nodes: [ClearSkyNode; 5],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ClearSkyNode {
    pub(crate) altitude_rad: f64,
    pub(crate) zenith_rgb: [f32; 3],
    pub(crate) horizon_rgb: [f32; 3],
    pub(crate) ambient_rgb: [f32; 3],
    pub(crate) exposure: f32,
    pub(crate) stars: f32,
    pub(crate) direct: f32,
}

const DEFAULT_CLEAR_SKY: [ClearSkyNode; 5] = [
    ClearSkyNode {
        altitude_rad: -18.0_f64.to_radians(),
        zenith_rgb: [0.0008, 0.0012, 0.0040],
        horizon_rgb: [0.0018, 0.0026, 0.0070],
        ambient_rgb: [0.0010, 0.0012, 0.0028],
        exposure: 1.25,
        stars: 1.0,
        direct: 0.0,
    },
    ClearSkyNode {
        altitude_rad: -12.0_f64.to_radians(),
        zenith_rgb: [0.0025, 0.0040, 0.0130],
        horizon_rgb: [0.0090, 0.0080, 0.0200],
        ambient_rgb: [0.0060, 0.0050, 0.0090],
        exposure: 1.15,
        stars: 0.5,
        direct: 0.0,
    },
    ClearSkyNode {
        altitude_rad: -6.0_f64.to_radians(),
        zenith_rgb: [0.0120, 0.0260, 0.0800],
        horizon_rgb: [0.2500, 0.0700, 0.0250],
        ambient_rgb: [0.0450, 0.0300, 0.0220],
        exposure: 1.05,
        stars: 0.0,
        direct: 0.0,
    },
    ClearSkyNode {
        altitude_rad: 0.0,
        zenith_rgb: [0.0750, 0.1500, 0.3200],
        horizon_rgb: [0.8500, 0.3000, 0.1000],
        ambient_rgb: [0.1200, 0.1050, 0.0900],
        exposure: 1.0,
        stars: 0.0,
        direct: 0.18,
    },
    ClearSkyNode {
        altitude_rad: 15.0_f64.to_radians(),
        zenith_rgb: [0.1500, 0.3600, 0.7800],
        horizon_rgb: [0.4800, 0.6900, 0.8800],
        ambient_rgb: [0.2300, 0.2650, 0.3000],
        exposure: 0.95,
        stars: 0.0,
        direct: 1.0,
    },
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SunlightProfile {
    pub(crate) global_rgb: [f32; 3],
    pub(crate) global_strength: f32,
    pub(crate) local_rgb: [f32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MoonlightProfile {
    pub(crate) local_rgb: [f32; 3],
    /// Full-Moon illumination coefficient relative to unit-sun irradiance.
    pub(crate) full_phase_strength: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HaloThresholds {
    pub(crate) emission: f64,
    pub(crate) reflection: f64,
    pub(crate) atmosphere: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HaloShapeProfile {
    pub(crate) sun_rgb: [f32; 3],
    pub(crate) moon_rgb: [f32; 3],
    pub(crate) sun_extent_rad: f64,
    pub(crate) moon_extent_rad: f64,
    pub(crate) body_extent_scale: f64,
    pub(crate) min_body_extent_rad: f64,
    pub(crate) max_body_extent_rad: f64,
    pub(crate) sun_importance: f32,
    pub(crate) moon_importance: f32,
    pub(crate) max_body_radiance: f64,
}

impl LightingProfile {
    pub(crate) const fn planetary() -> Self {
        Self {
            sunlight: SunlightProfile {
                global_rgb: [1.0, 0.97, 0.90],
                global_strength: 1.0,
                local_rgb: [1.0, 0.97, 0.90],
            },
            moonlight: MoonlightProfile {
                local_rgb: [0.25, 0.32, 0.50],
                full_phase_strength: 2.0e-6,
            },
            ambient: [1.0, 1.0, 1.0],
            exposure: 1.0,
            halo_thresholds: HaloThresholds {
                emission: 1.0e-4,
                reflection: 1.0e-4,
                atmosphere: 1.0e-4,
            },
            halos: HaloShapeProfile {
                sun_rgb: [0.32, 0.15, 0.045],
                moon_rgb: [0.09, 0.12, 0.18],
                sun_extent_rad: 8.0_f64.to_radians(),
                moon_extent_rad: 4.0_f64.to_radians(),
                body_extent_scale: 12.0,
                min_body_extent_rad: 0.3_f64.to_radians(),
                max_body_extent_rad: 4.0_f64.to_radians(),
                sun_importance: 1.0,
                moon_importance: 0.5,
                max_body_radiance: 10_000.0,
            },
            clear_sky: ClearSkyProfile {
                nodes: DEFAULT_CLEAR_SKY,
            },
        }
    }

    pub(crate) const fn interior() -> Self {
        let planetary = Self::planetary();
        Self {
            sunlight: SunlightProfile {
                global_rgb: [0.0; 3],
                global_strength: 0.0,
                local_rgb: [0.0; 3],
            },
            moonlight: MoonlightProfile {
                local_rgb: [0.0; 3],
                full_phase_strength: 0.0,
            },
            ambient: [0.32, 0.32, 0.32],
            exposure: 1.0,
            halo_thresholds: planetary.halo_thresholds,
            halos: planetary.halos,
            clear_sky: planetary.clear_sky,
        }
    }

    pub(crate) fn validate(self) -> Result<(), ()> {
        let mut colors = self
            .sunlight
            .global_rgb
            .iter()
            .chain(self.sunlight.local_rgb.iter())
            .chain(self.moonlight.local_rgb.iter())
            .chain(self.ambient.iter())
            .chain(self.halos.sun_rgb.iter())
            .chain(self.halos.moon_rgb.iter());
        let thresholds = [
            self.halo_thresholds.emission,
            self.halo_thresholds.reflection,
            self.halo_thresholds.atmosphere,
        ];
        let halo_values = [
            self.halos.sun_extent_rad,
            self.halos.moon_extent_rad,
            self.halos.body_extent_scale,
            self.halos.min_body_extent_rad,
            self.halos.max_body_extent_rad,
            self.halos.max_body_radiance,
        ];
        let clear_sky_valid = self.clear_sky.nodes.iter().all(|node| {
            node.altitude_rad.is_finite()
                && node
                    .zenith_rgb
                    .iter()
                    .all(|value| value.is_finite() && (0.0..=MAX_PROFILE_RGB).contains(value))
                && node
                    .horizon_rgb
                    .iter()
                    .all(|value| value.is_finite() && (0.0..=MAX_PROFILE_RGB).contains(value))
                && node
                    .ambient_rgb
                    .iter()
                    .all(|value| value.is_finite() && (0.0..=MAX_PROFILE_RGB).contains(value))
                && node.exposure.is_finite()
                && (0.0..=MAX_PROFILE_EXPOSURE).contains(&node.exposure)
                && node.stars.is_finite()
                && (0.0..=1.0).contains(&node.stars)
                && node.direct.is_finite()
                && (0.0..=1.0).contains(&node.direct)
        });
        let valid = colors
            .all(|value| value.is_finite() && (0.0..=MAX_PROFILE_RGB).contains(value))
            && self.sunlight.global_strength.is_finite()
            && (0.0..=MAX_LIGHT_STRENGTH).contains(&self.sunlight.global_strength)
            && self.moonlight.full_phase_strength.is_finite()
            && (0.0..=1.0).contains(&self.moonlight.full_phase_strength)
            && self.exposure.is_finite()
            && (0.0..=MAX_PROFILE_EXPOSURE).contains(&self.exposure)
            && self.exposure > 0.0
            && thresholds
                .iter()
                .all(|value| value.is_finite() && (0.0..=MAX_PROFILE_THRESHOLD).contains(value))
            && halo_values
                .iter()
                .all(|value| value.is_finite() && *value > 0.0)
            && self.halos.sun_extent_rad <= std::f64::consts::FRAC_PI_2
            && self.halos.moon_extent_rad <= std::f64::consts::FRAC_PI_2
            && self.halos.min_body_extent_rad <= self.halos.max_body_extent_rad
            && self.halos.body_extent_scale <= 1_000.0
            && self.halos.max_body_radiance <= MAX_PROFILE_THRESHOLD
            && self.halos.sun_importance.is_finite()
            && (0.0..=MAX_PROFILE_RGB).contains(&self.halos.sun_importance)
            && self.halos.moon_importance.is_finite()
            && (0.0..=MAX_PROFILE_RGB).contains(&self.halos.moon_importance)
            && clear_sky_valid
            && self
                .clear_sky
                .nodes
                .windows(2)
                .all(|pair| pair[0].altitude_rad < pair[1].altitude_rad);
        if valid {
            Ok(())
        } else {
            Err(())
        }
    }
}

impl Default for LightingProfile {
    fn default() -> Self {
        Self::planetary()
    }
}
