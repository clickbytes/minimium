use super::environment::{EnvironmentFrame, EnvironmentFrameStatus, LocalBodyFrame};
use glam::DVec3;
use minimium_renderer::lighting::luminance;
pub(crate) use minimium_renderer::shadow_uniform::ShadowUniform;
use minimium_renderer::AtmosphereUniform;
const SHADOW_SOURCE_SUN: u32 = 1;
const SHADOW_SOURCE_MOON: u32 = 2;
const SHADOW_SOURCE_BOLID: u32 = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ShadowSource {
    Sun,
    Moon,
    Bolid,
}

impl ShadowSource {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Sun => "sun",
            Self::Moon => "moon",
            Self::Bolid => "bolid",
        }
    }

    pub(super) fn id(self) -> u32 {
        match self {
            Self::Sun => SHADOW_SOURCE_SUN,
            Self::Moon => SHADOW_SOURCE_MOON,
            Self::Bolid => SHADOW_SOURCE_BOLID,
        }
    }

    pub(super) fn direction(
        self,
        frame: &EnvironmentFrame,
        air: Option<&AtmosphereUniform>,
        point: DVec3,
    ) -> Option<DVec3> {
        let Some(air) = air else {
            let body = match self {
                Self::Sun => frame.local_sun?,
                Self::Moon => frame.local_moon?,
                Self::Bolid => return None,
            };
            return DVec3::from_array(body.apparent_direction_world).try_normalize();
        };
        let direction = match self {
            Self::Sun => DVec3::from_array(frame.global_sun?.direction_world).try_normalize()?,
            Self::Moon => {
                let [x, y, z, radius] = frame.environment_uniform.moon_orbit_world;
                if radius <= 0.0 {
                    return None;
                }
                (DVec3::new(x.into(), y.into(), z.into()) - point).try_normalize()?
            }
            Self::Bolid => return None,
        };
        let up = point.try_normalize()?;
        let height =
            ((point.length() - f64::from(air.config[0])) * f64::from(air.config[1])).max(0.0);
        let cosine = up.dot(direction).clamp(-1.0, 1.0);
        let tangent = up - direction * cosine;
        if height >= f64::from(air.radii[1])
            || air.radii[2] <= 0.0
            || tangent.length_squared() < 1e-8
        {
            return Some(direction);
        }
        let lift = minimium_astronomy::standard_refraction_rad(cosine.asin()).ok()?
            * (-height / f64::from(air.radii[2])).exp();
        Some(direction * lift.cos() + tangent.normalize() * lift.sin())
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
    atmosphere: Option<&AtmosphereUniform>,
) -> (Option<ShadowSource>, bool) {
    if frame.status != EnvironmentFrameStatus::Available {
        return (None, false);
    }
    let (sun, moon) = if let Some(air) = atmosphere {
        let mut points = minimium_renderer::shadow_view::surface_footprint(frame.render_view);
        points.push(glam::DVec3::from_array(center));
        let strength = |source: ShadowSource, rgb: [f32; 3]| -> f32 {
            let solar_radius = frame
                .local_sun
                .map_or(0.0, |sun| sun.angular_radius_rad.sin());
            let [x, y, z, r] = frame.environment_uniform.moon_orbit_world;
            let moon_center = DVec3::new(x.into(), y.into(), z.into());
            let maximum = points
                .iter()
                .filter_map(|&point| {
                    let direction = source.direction(frame, Some(air), point)?;
                    let cosine = point.try_normalize()?.dot(direction);
                    let radius_ratio = (f64::from(air.config[0]) / point.length()).min(1.0);
                    let horizon = -(1.0 - radius_ratio * radius_ratio).sqrt();
                    let radius = if source == ShadowSource::Moon {
                        (f64::from(r) / point.distance(moon_center)).clamp(1e-7, 1.0)
                    } else {
                        solar_radius
                    };
                    if radius <= 0.0 {
                        return None;
                    }
                    let half_width = (radius * radius_ratio).max(1e-7);
                    let t = ((cosine - horizon + half_width) / (2.0 * half_width)).clamp(0.0, 1.0);
                    Some(t * t * (3.0 - 2.0 * t) * cosine.max(0.02))
                })
                .fold(0.0, f64::max);
            luminance(rgb) * maximum as f32
        };
        let sun_rgb = frame.environment_uniform.global_sun_rgb;
        let moon_rgb = frame.environment_uniform.global_moon_rgb;
        (
            strength(ShadowSource::Sun, [sun_rgb[0], sun_rgb[1], sun_rgb[2]]),
            strength(ShadowSource::Moon, [moon_rgb[0], moon_rgb[1], moon_rgb[2]]),
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
    };
    let Some(strongest) = strongest else {
        return (None, false);
    };
    let strongest_value = match strongest {
        ShadowSource::Sun => sun,
        ShadowSource::Moon => moon,
        ShadowSource::Bolid => 0.0,
    };
    let selected = previous
        .filter(|source| {
            let value = match source {
                ShadowSource::Sun => sun,
                ShadowSource::Moon => moon,
                ShadowSource::Bolid => 0.0,
            };
            value > 0.0 && value >= strongest_value * 0.95
        })
        .or(Some(strongest));
    (selected, moon > 0.0)
}

fn body_strength(body: Option<LocalBodyFrame>) -> f32 {
    let Some(body) = body else {
        return 0.0;
    };
    luminance(body.effective_linear_rgb)
}

// The view footprint samples sea level. A published peak can see the Moon
// after every such sample has entered the planetary shadow. Bounding corners
// conservatively retain the map until all candidate receivers are dark.
pub(super) fn published_moonlight(
    frame: &EnvironmentFrame,
    air: Option<&minimium_renderer::AtmosphereUniform>,
    region: minimium_renderer::shadow_view::ShadowRegion,
    mut bounds: impl Iterator<Item = minimium_renderer::shadow_geometry::ShadowBounds>,
) -> bool {
    let Some(air) = air else {
        return false;
    };
    let env = &frame.environment_uniform;
    if !env.global_moon_rgb[..3].iter().any(|v| *v > 0.0) {
        return false;
    }
    let [x, y, z, r] = env.moon_orbit_world;
    let moon = glam::DVec3::new(x.into(), y.into(), z.into());
    let radius = f64::from(air.config[0]);
    // An enclosing sphere bounds receiver height, radial direction and parallax.
    bounds.any(|bounds| {
        if !bounds.intersects_sphere(
            region.center_world,
            region.settings.receiver_radius_m.into(),
        ) {
            return false;
        }
        let lo = glam::DVec3::from_array(bounds.minimum);
        let hi = glam::DVec3::from_array(bounds.maximum);
        let center = (lo + hi) * 0.5;
        let extent = (hi - lo).length() * 0.5;
        let distance = center.length();
        if distance <= extent {
            return true;
        }
        let ray = moon - center;
        let zenith = (center / distance).angle_between(ray);
        let dip = (radius / (distance + extent)).clamp(0.0, 1.0).acos();
        let spread =
            (extent / distance).asin() + ((extent + f64::from(r)) / ray.length()).min(1.0).asin();
        zenith < std::f64::consts::FRAC_PI_2 + dip + spread + 1.0_f64.to_radians()
    })
}
