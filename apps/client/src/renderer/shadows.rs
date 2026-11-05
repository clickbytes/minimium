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

    pub(super) fn direction(self, frame: &EnvironmentFrame) -> Option<[f64; 3]> {
        let direction = match self {
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
) -> Option<ShadowSource> {
    if frame.status != EnvironmentFrameStatus::Available {
        return None;
    }
    let sun = body_strength(frame.local_sun);
    let moon = body_strength(frame.local_moon);
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
    let [red, green, blue] = body.effective_linear_rgb;
    (0.2126 * red + 0.7152 * green + 0.0722 * blue).max(0.0)
}
