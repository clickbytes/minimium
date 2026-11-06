use super::environment::{EnvironmentFrame, EnvironmentFrameStatus, LocalBodyFrame};
pub(crate) use minimium_renderer::shadow_uniform::ShadowUniform;
const SHADOW_SOURCE_SUN: u32 = 1;
const SHADOW_SOURCE_MOON: u32 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ShadowSource {
    Sun,
    Moon,
}

impl ShadowSource {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Sun => "sun",
            Self::Moon => "moon",
        }
    }

    pub(super) fn id(self) -> u32 {
        match self {
            Self::Sun => SHADOW_SOURCE_SUN,
            Self::Moon => SHADOW_SOURCE_MOON,
        }
    }

    pub(super) fn direction(self, frame: &EnvironmentFrame, physical: bool) -> Option<[f64; 3]> {
        let direction = match self {
            Self::Sun if physical => frame.global_sun?.direction_world,
            Self::Moon if physical => std::array::from_fn(|i| {
                f64::from(frame.environment_uniform.moon_orbit_world[i])
                    - frame.render_view.camera_eye_world_m[i]
            }),
            Self::Sun => frame.local_sun?.geometric_direction_world,
            Self::Moon => frame.local_moon?.geometric_direction_world,
        };
        let length = direction
            .iter()
            .map(|value| value * value)
            .sum::<f64>()
            .sqrt();
        (length.is_finite() && length > 1.0e-12).then(|| direction.map(|value| value / length))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ShadowInvalidReason {
    Disabled,
    NoSource,
    InvalidFrame,
    IncompleteCasters,
    BudgetExceeded,
    StaleContext,
}

impl ShadowInvalidReason {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::NoSource => "no_source",
            Self::InvalidFrame => "invalid_frame",
            Self::IncompleteCasters => "incomplete_casters",
            Self::BudgetExceeded => "budget_exceeded",
            Self::StaleContext => "stale_context",
        }
    }
}

#[derive(Default)]
pub(super) struct ShadowState {
    pub(super) previous_source: Option<ShadowSource>,
    pub(super) source: Option<ShadowSource>,
    pub(super) valid: bool,
    pub(super) receiver_radius_m: f32,
    pub(super) texel_m: f32,
    pub(super) invalid_reason: Option<ShadowInvalidReason>,
    pub(super) caster_keys: Vec<(minimium_world::world::SectorKey, u8)>,
}

impl ShadowState {
    pub(super) fn invalidate(&mut self, reason: ShadowInvalidReason) {
        self.valid = false;
        self.source = None;
        self.caster_keys.clear();
        self.invalid_reason = Some(reason);
    }
}

pub(super) fn select_source(
    frame: &EnvironmentFrame,
    previous: Option<ShadowSource>,
    center: [f64; 3],
    physical: bool,
) -> Option<ShadowSource> {
    if frame.status != EnvironmentFrameStatus::Available {
        return None;
    }
    let (sun, moon) = if physical {
        let mut points = minimium_renderer::shadow_view::surface_footprint(frame.render_view);
        points.push(glam::DVec3::from_array(center));
        let strength = |source: ShadowSource, rgb: [f32; 3]| -> f32 {
            let Some(direction) = source.direction(frame, true) else {
                return 0.0;
            };
            let direction = glam::DVec3::from_array(direction);
            let cosine = points
                .iter()
                .map(|point| point.normalize_or_zero().dot(direction))
                .fold(-1.0, f64::max);
            let disk = ((cosine + 0.00465) / 0.0093).clamp(0.0, 1.0) as f32;
            luminance(rgb) * disk * cosine.max(0.02) as f32
        };
        let sun_rgb = frame.environment_uniform.global_sun_rgb;
        let flux = frame.environment_uniform.moon_disk_radiance[3];
        (
            strength(ShadowSource::Sun, [sun_rgb[0], sun_rgb[1], sun_rgb[2]]),
            strength(
                ShadowSource::Moon,
                [0.25, 0.32, 0.5].map(|v| v * 2e-6 * flux),
            ),
        )
    } else {
        (
            body_strength(frame.local_sun),
            body_strength(frame.local_moon),
        )
    };
    let strongest = if sun > 0.0 || moon > 0.0 {
        Some(if sun >= moon {
            ShadowSource::Sun
        } else {
            ShadowSource::Moon
        })
    } else {
        None
    }?;
    let strongest_value = match strongest {
        ShadowSource::Sun => sun,
        ShadowSource::Moon => moon,
    };
    previous
        .filter(|source| {
            let value = match source {
                ShadowSource::Sun => sun,
                ShadowSource::Moon => moon,
            };
            value > 0.0 && value >= strongest_value * 0.95
        })
        .or(Some(strongest))
}

fn body_strength(body: Option<LocalBodyFrame>) -> f32 {
    let Some(body) = body else {
        return 0.0;
    };
    luminance(body.effective_linear_rgb)
}

fn luminance([red, green, blue]: [f32; 3]) -> f32 {
    (0.2126 * red + 0.7152 * green + 0.0722 * blue).max(0.0)
}
