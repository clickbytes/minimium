use super::{HaloSource, LocalBodyFrame, RenderViewFrame, MAX_HALO_BODIES};
use bytemuck::Zeroable;
use minimium_astronomy::{ObservedBody, SKY_MOON_ID, SKY_SUN_ID};
use std::f64::consts::PI;

const SUN_PROFILE: u32 = 0;
const MOON_PROFILE: u32 = 1;
const EMISSION_PROFILE: u32 = 2;
const REFLECTION_PROFILE: u32 = 3;
const ATMOSPHERE_PROFILE: u32 = 4;

type HaloPreparation = (
    Vec<ObservedBody>,
    [HaloSource; MAX_HALO_BODIES],
    u32,
    [f32; 4],
);

pub(super) fn prepare_halos(
    sky: &minimium_astronomy::SkyState,
    view: RenderViewFrame,
    sun: LocalBodyFrame,
    moon: LocalBodyFrame,
    moon_source: [f64; 3],
) -> Result<HaloPreparation, super::EnvironmentFrameError> {
    let moon_orbit_world = super::orbit::moon_orbit_world(sky, view)?;
    let bodies = minimium_astronomy::observed_bodies(sky, None);
    let overview_moon_direction = std::array::from_fn(|axis| {
        f64::from(moon_orbit_world[axis]) - view.camera_eye_world_m[axis]
    });
    let (sources, count) = build_halos(
        &bodies,
        view,
        sun,
        moon,
        sky.geocentric_sun_direction_world,
        overview_moon_direction,
        moon_source,
    );
    Ok((bodies, sources, count, moon_orbit_world))
}

pub(super) fn build_halos(
    bodies: &[ObservedBody],
    view: RenderViewFrame,
    sun: LocalBodyFrame,
    moon: LocalBodyFrame,
    overview_sun_direction: [f64; 3],
    overview_moon_direction: [f64; 3],
    moon_source: [f64; 3],
) -> ([HaloSource; MAX_HALO_BODIES], u32) {
    let mut candidates = bodies
        .iter()
        .filter_map(|body| {
            source_for(
                *body,
                view,
                sun,
                moon,
                overview_sun_direction,
                overview_moon_direction,
                moon_source,
            )
        })
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

fn source_for(
    body: ObservedBody,
    view: RenderViewFrame,
    sun: LocalBodyFrame,
    moon: LocalBodyFrame,
    overview_sun_direction: [f64; 3],
    overview_moon_direction: [f64; 3],
    moon_source: [f64; 3],
) -> Option<(u64, f32, HaloSource)> {
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
            SUN_PROFILE,
            1.0,
            [0.32, 0.15, 0.045],
            8.0_f64.to_radians(),
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
            MOON_PROFILE,
            body.phase.clamp(0.0, 1.0) as f32,
            [0.09, 0.12, 0.18],
            4.0_f64.to_radians(),
        )
    } else {
        if !body.visible || !body.phase.is_finite() {
            return None;
        }
        let emission = body.emitted_linear_rgb;
        let reflection = body.reflected_linear_rgb.map(|value| value * body.phase);
        let scattering = body.scattered_linear_rgb;
        let (color, shape) = dominant_profile(emission, reflection, scattering)?;
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
        let extent =
            (body.angular_radius_rad * 12.0).clamp(0.3_f64.to_radians(), 4.0_f64.to_radians());
        let source_direction = if shape == REFLECTION_PROFILE || shape == ATMOSPHERE_PROFILE {
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
        1.0
    } else if is_moon {
        0.5 * visibility
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
        parameters: [shape as f32, phase, 0.0, 0.0],
    };
    Some((body.id.get(), importance, source))
}

fn dominant_profile(
    emission: [f64; 3],
    reflection: [f64; 3],
    scattering: [f64; 3],
) -> Option<([f32; 3], u32)> {
    let candidates = [
        (emission, EMISSION_PROFILE),
        (reflection, REFLECTION_PROFILE),
        (scattering, ATMOSPHERE_PROFILE),
    ];
    candidates
        .into_iter()
        .filter_map(|(rgb, shape)| {
            let rgb = rgb.map(|value| value.max(0.0));
            let brightness = max_component(rgb);
            (brightness > 1.0e-4 && brightness.is_finite()).then_some((rgb, shape, brightness))
        })
        .max_by(|a, b| a.2.total_cmp(&b.2))
        .and_then(|(rgb, shape, _)| {
            let color = rgb.map(|value| value.min(10_000.0) as f32);
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

fn max_component(rgb: [f64; 3]) -> f32 {
    rgb.into_iter()
        .fold(0.0_f64, f64::max)
        .min(f64::from(f32::MAX)) as f32
}

fn to_f32(value: f64) -> Option<f32> {
    (value.is_finite() && value.abs() <= f64::from(f32::MAX)).then_some(value as f32)
}
