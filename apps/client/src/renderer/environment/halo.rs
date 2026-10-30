use super::{EnvironmentSettings, HaloSource, LocalBodyFrame, RenderViewFrame, MAX_HALO_BODIES};
use bytemuck::Zeroable;
use minimium_astronomy::{ObservedBody, SKY_MOON_ID, SKY_SUN_ID};
use std::f64::consts::PI;

#[derive(Clone, Copy, Eq, PartialEq)]
#[repr(u32)]
enum HaloShape {
    Sun = 0,
    Moon = 1,
    Emission = 2,
    Reflection = 3,
    Atmosphere = 4,
}

type HaloPreparation = ([HaloSource; MAX_HALO_BODIES], u32, [f32; 4]);

#[derive(Clone, Copy)]
struct HaloContext {
    view: RenderViewFrame,
    sun: LocalBodyFrame,
    moon: LocalBodyFrame,
    overview_sun_direction: [f64; 3],
    overview_moon_direction: [f64; 3],
    moon_source: [f64; 3],
    profile: EnvironmentSettings,
}

pub(super) fn prepare_halos(
    sky: &minimium_astronomy::SkyState,
    view: RenderViewFrame,
    sun: LocalBodyFrame,
    moon: LocalBodyFrame,
    moon_source: [f64; 3],
    bodies: &[ObservedBody],
    profile: EnvironmentSettings,
) -> Result<HaloPreparation, super::EnvironmentFrameError> {
    let moon_orbit_world = super::orbit::moon_orbit_world(sky, view)?;
    let overview_moon_direction = std::array::from_fn(|axis| {
        f64::from(moon_orbit_world[axis]) - view.camera_eye_world_m[axis]
    });
    let context = HaloContext {
        view,
        sun,
        moon,
        overview_sun_direction: sky.geocentric_sun_direction_world,
        overview_moon_direction,
        moon_source,
        profile,
    };
    let (sources, count) = build_halos(bodies, context);
    Ok((sources, count, moon_orbit_world))
}

fn build_halos(
    bodies: &[ObservedBody],
    context: HaloContext,
) -> ([HaloSource; MAX_HALO_BODIES], u32) {
    let mut candidates = bodies
        .iter()
        .filter_map(|body| source_for(*body, context))
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .1
            .total_cmp(&left.1)
            .then_with(|| left.0.cmp(&right.0))
    });

    let count = candidates.len().min(MAX_HALO_BODIES);
    let mut sources = [HaloSource::zeroed(); MAX_HALO_BODIES];
    for (slot, (_, _, source)) in candidates.into_iter().take(count).enumerate() {
        sources[slot] = source;
    }
    (sources, count as u32)
}

fn source_for(body: ObservedBody, context: HaloContext) -> Option<(u64, f32, HaloSource)> {
    let HaloContext {
        view,
        sun,
        moon,
        overview_sun_direction,
        overview_moon_direction,
        moon_source,
        profile,
    } = context;
    let is_sun = body.id == SKY_SUN_ID;
    let is_moon = body.id == SKY_MOON_ID;
    let (direction, visibility, source_direction, shape, phase, color, extent) = if is_sun {
        (
            if view.kind == super::EnvironmentViewKind::Overview {
                overview_sun_direction
            } else {
                sun.apparent_direction_world
            },
            if view.kind == super::EnvironmentViewKind::Overview {
                1.0
            } else {
                sun.visible_fraction
            },
            [0.0; 3],
            HaloShape::Sun,
            1.0,
            profile.halos.sun_rgb,
            profile.halos.sun_extent_rad,
        )
    } else if is_moon {
        (
            if view.kind == super::EnvironmentViewKind::Overview {
                overview_moon_direction
            } else {
                moon.apparent_direction_world
            },
            if view.kind == super::EnvironmentViewKind::Overview {
                1.0
            } else {
                moon.visible_fraction
            },
            moon_source,
            HaloShape::Moon,
            body.phase.clamp(0.0, 1.0) as f32,
            profile.halos.moon_rgb,
            profile.halos.moon_extent_rad,
        )
    } else {
        if !body.visible || !body.phase.is_finite() {
            return None;
        }
        let emission = body.emitted_linear_rgb;
        let reflection = body.reflected_linear_rgb.map(|value| value * body.phase);
        let scattering = body.scattered_linear_rgb;
        let (color, shape) = dominant_profile(emission, reflection, scattering, profile)?;
        let up = normalize(view.camera_eye_world_m)?;
        let direction = normalize(body.direction_world)?;
        let altitude = dot(direction, up).clamp(-1.0, 1.0).asin();
        let horizon = if view.kind == super::EnvironmentViewKind::Overview {
            0.0
        } else {
            view_horizon_altitude(view)
        };
        let visibility = if view.kind == super::EnvironmentViewKind::Overview {
            1.0
        } else {
            visible_disk_fraction(altitude, body.angular_radius_rad, horizon)
        };
        let extent = (body.angular_radius_rad * profile.halos.body_extent_scale).clamp(
            profile.halos.min_body_extent_rad,
            profile.halos.max_body_extent_rad,
        );
        let source_direction = if shape == HaloShape::Reflection || shape == HaloShape::Atmosphere {
            overview_sun_direction
        } else {
            [0.0; 3]
        };
        (
            direction,
            visibility,
            source_direction,
            shape,
            body.phase.clamp(0.0, 1.0) as f32,
            color,
            extent,
        )
    };
    if !body.visible && !is_sun && !is_moon {
        return None;
    }
    let direction = normalize(direction)?;
    let source_direction = normalize_or_zero(source_direction);
    if !visibility.is_finite() || visibility <= 0.0 {
        return None;
    }
    let visibility = visibility.clamp(0.0, 1.0);
    let importance = if is_sun {
        profile.halos.sun_importance
    } else if is_moon {
        profile.halos.moon_importance * visibility
    } else {
        color.into_iter().fold(0.0_f32, f32::max) * visibility
    };
    let source = HaloSource {
        direction_extent: [
            to_f32(direction[0])?,
            to_f32(direction[1])?,
            to_f32(direction[2])?,
            extent as f32,
        ],
        linear_rgb_visibility: [color[0], color[1], color[2], visibility],
        source_direction: [
            source_direction[0] as f32,
            source_direction[1] as f32,
            source_direction[2] as f32,
            0.0,
        ],
        parameters: [shape as u32 as f32, phase, 0.0, 0.0],
    };
    Some((body.id.get(), importance, source))
}

fn dominant_profile(
    emission: [f64; 3],
    reflection: [f64; 3],
    scattering: [f64; 3],
    profile: EnvironmentSettings,
) -> Option<([f32; 3], HaloShape)> {
    let candidates = [
        (
            emission,
            profile.halo_thresholds.emission,
            HaloShape::Emission,
        ),
        (
            reflection,
            profile.halo_thresholds.reflection,
            HaloShape::Reflection,
        ),
        (
            scattering,
            profile.halo_thresholds.atmosphere,
            HaloShape::Atmosphere,
        ),
    ];
    candidates
        .into_iter()
        .filter_map(|(rgb, threshold, shape)| {
            let rgb = rgb.map(|value| value.max(0.0));
            let brightness = max_component(rgb);
            (brightness > threshold && brightness.is_finite()).then_some((rgb, shape, brightness))
        })
        .max_by(|a, b| a.2.total_cmp(&b.2))
        .and_then(|(rgb, shape, _)| {
            let color = rgb.map(|value| value.min(profile.halos.max_body_radiance) as f32);
            color
                .iter()
                .all(|value| value.is_finite())
                .then_some((color, shape))
        })
}

pub(super) fn visible_disk_fraction(altitude: f64, radius: f64, horizon: f64) -> f32 {
    if !altitude.is_finite() || !radius.is_finite() || radius <= 0.0 {
        return 0.0;
    }
    let elevation = altitude - horizon;
    if elevation <= -radius {
        return 0.0;
    }
    if elevation >= radius {
        return 1.0;
    }
    let x = (elevation / radius).clamp(-1.0, 1.0);
    (0.5 + (x.asin() + x * (1.0 - x * x).max(0.0).sqrt()) / PI) as f32
}

fn view_horizon_altitude(view: RenderViewFrame) -> f64 {
    let radius = view.game_planet_radius_m;
    let eye_radius = length(view.camera_eye_world_m);
    if radius <= 0.0 || eye_radius <= 0.0 || !eye_radius.is_finite() || eye_radius < radius {
        return 0.0;
    }
    -((1.0 - (radius / eye_radius).powi(2)).max(0.0).sqrt()).asin()
}

fn normalize(vector: [f64; 3]) -> Option<[f64; 3]> {
    let length = length(vector);
    (length.is_finite() && length > 1.0e-15).then(|| vector.map(|value| value / length))
}

fn normalize_or_zero(vector: [f64; 3]) -> [f64; 3] {
    normalize(vector).unwrap_or([0.0; 3])
}

fn length(vector: [f64; 3]) -> f64 {
    vector.iter().map(|value| value * value).sum::<f64>().sqrt()
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|axis| a[axis] * b[axis]).sum()
}

fn max_component(rgb: [f64; 3]) -> f64 {
    rgb.into_iter().fold(0.0_f64, f64::max)
}

fn to_f32(value: f64) -> Option<f32> {
    (value.is_finite() && value.abs() <= f64::from(f32::MAX)).then_some(value as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimium_core::universe::{BodyKind, CelestialBodyId};

    #[test]
    fn halo_upload_keeps_the_eight_brightest_bodies() {
        let bodies = (3..=12)
            .map(|id| ObservedBody {
                id: CelestialBodyId::new(id).unwrap(),
                kind: BodyKind::Star,
                direction_world: [0.0, 1.0, 0.0],
                distance_m: 1.0e9,
                angular_radius_rad: 0.01,
                phase: 1.0,
                emitted_linear_rgb: [id as f64, 0.0, 0.0],
                reflected_linear_rgb: [0.0; 3],
                scattered_linear_rgb: [0.0; 3],
                visible: true,
            })
            .collect::<Vec<_>>();
        let dummy = LocalBodyFrame {
            geometric_direction_world: [0.0, 1.0, 0.0],
            apparent_direction_world: [0.0, 1.0, 0.0],
            geometric_altitude_rad: 0.0,
            apparent_altitude_rad: 0.0,
            angular_radius_rad: 0.01,
            distance_m: 1.0e9,
            visible_fraction: 1.0,
            effective_linear_rgb: [1.0; 3],
        };
        let context = HaloContext {
            view: RenderViewFrame::default(),
            sun: dummy,
            moon: dummy,
            overview_sun_direction: [0.0, 1.0, 0.0],
            overview_moon_direction: [0.0, 1.0, 0.0],
            moon_source: [0.0, 1.0, 0.0],
            profile: EnvironmentSettings::default(),
        };
        let (sources, count) = build_halos(&bodies, context);
        assert_eq!(count, 8);
        let brightness = sources.map(|source| source.linear_rgb_visibility[0]);
        assert_eq!(brightness, [12.0, 11.0, 10.0, 9.0, 8.0, 7.0, 6.0, 5.0]);
    }
}
