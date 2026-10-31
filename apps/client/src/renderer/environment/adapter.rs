use super::super::environment_gpu::{camera_uniform, EnvironmentUniform};
use super::{
    EnvironmentFrame, EnvironmentFrameDiagnostics, EnvironmentFrameStatus, EnvironmentSettings,
    GlobalSunFrame, RenderViewFrame, SkyFrameInput,
};
use bytemuck::Zeroable;
use minimium_astronomy::{SkyState, ASTRONOMY_MODEL_REVISION};
use minimium_core::{
    environment::{EnvironmentDescriptor, EnvironmentProfile},
    sky::SkyModel,
    time::{ClockId, ClockRevision},
};
use minimium_protocol::WorldContext;
use std::{error::Error, f64::consts::PI, fmt};

// Presentation scale for sky disks; physical angular radii still drive lighting and visibility.
const SKY_DISK_DISPLAY_SCALE: f64 = 1.8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EnvironmentFrameError {
    DescriptorMismatch,
    SkyStateInvalid,
    ObserverMismatch,
    SettingsInvalid,
}

impl fmt::Display for EnvironmentFrameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::DescriptorMismatch => "environment descriptor is incompatible with the sky",
            Self::SkyStateInvalid => "astronomy returned an invalid sky state",
            Self::ObserverMismatch => "observer and sky model revisions differ",
            Self::SettingsInvalid => "environment settings contain an invalid value",
        };
        formatter.write_str(message)
    }
}

impl Error for EnvironmentFrameError {}

pub(super) fn from_sky_state(
    sky: &SkyState,
    input: SkyFrameInput,
) -> Result<EnvironmentFrame, EnvironmentFrameError> {
    let SkyFrameInput {
        observer,
        context,
        clock_revision,
        descriptor,
        render_view,
        observed_bodies,
        settings,
    } = input;
    if descriptor.profile() == EnvironmentProfile::Interior
        || descriptor.astronomy_model_revision().get() != ASTRONOMY_MODEL_REVISION
        || descriptor.validate_clock(ClockId::MVP).is_err()
    {
        return Err(EnvironmentFrameError::DescriptorMismatch);
    }
    let profile_matches_model = matches!(
        (descriptor.profile(), observer.model),
        (EnvironmentProfile::Earth, SkyModel::EarthGeoreferencedV1)
            | (EnvironmentProfile::EarthLike, SkyModel::MappedEarthSkyV1)
    );
    if !profile_matches_model || sky.observer_model_revision != observer.height_reference_revision {
        return Err(EnvironmentFrameError::ObserverMismatch);
    }
    if !render_view.is_finite()
        || settings.validate().is_err()
        || !valid_body(sky.sun)
        || !valid_body(sky.moon.body)
        || !sky.moon.illuminated_fraction.is_finite()
        || !(0.0..=1.0).contains(&sky.moon.illuminated_fraction)
        || !sky.moon.apparent_v_magnitude.is_finite()
        || !sky.moon.phase_angle_rad.is_finite()
        || !valid_vector(sky.moon.moon_to_sun_world)
        || !valid_vector(sky.moon.pole_world)
        || !valid_vector(sky.geocentric_sun_direction_world)
        || !valid_vector(sky.geocentric_moon_position_world_m)
        || !valid_vector(sky.sun.geometric_direction_world)
        || !valid_vector(sky.sun.apparent_direction_world)
        || !valid_vector(sky.moon.body.geometric_direction_world)
        || !valid_vector(sky.moon.body.apparent_direction_world)
        || !sky
            .catalog_epoch_to_world
            .iter()
            .flatten()
            .all(|value| value.is_finite())
    {
        return Err(EnvironmentFrameError::SkyStateInvalid);
    }

    let (observer_up, horizon_cos, below_radius) = view_horizon(render_view);
    let horizon_altitude = horizon_cos.asin();
    let sun_visible = super::halo::visible_disk_fraction(
        sky.sun.apparent_altitude_rad,
        sky.sun.angular_radius_rad,
        horizon_altitude,
    );
    let moon_visible = super::halo::visible_disk_fraction(
        sky.moon.body.apparent_altitude_rad,
        sky.moon.body.angular_radius_rad,
        horizon_altitude,
    );
    let clear_sky =
        super::super::lighting::evaluate_clear_sky_parameters(sky, &observer, &settings);
    let disk_sun = super::body_frame(
        sky.sun,
        settings.sunlight.local_rgb,
        clear_sky.direct_sun_factor,
        sun_visible,
    );
    let moon_flux = super::super::moon_lighting::apparent_moon_flux(sky.moon.apparent_v_magnitude)
        .ok_or(EnvironmentFrameError::SkyStateInvalid)?;
    let moon_scale = super::super::moon_lighting::lunar_irradiance_at_observer(
        moon_flux,
        sky.moon.body.apparent_altitude_rad,
        sky.moon.body.angular_radius_rad,
        observer.geographic.height_m(),
        settings.moonlight.full_phase_strength,
    )
    .ok_or(EnvironmentFrameError::SkyStateInvalid)?;
    let disk_moon = super::body_frame(
        sky.moon.body,
        settings.moonlight.local_rgb,
        moon_scale,
        moon_visible,
    );
    let moon_to_sun = rotate_shortest_arc(
        sky.moon.body.geometric_direction_world,
        sky.moon.body.apparent_direction_world,
        sky.moon.moon_to_sun_world,
    )
    .ok_or(EnvironmentFrameError::SkyStateInvalid)?;
    let moon_pole = rotate_shortest_arc(
        sky.moon.body.geometric_direction_world,
        sky.moon.body.apparent_direction_world,
        sky.moon.pole_world,
    )
    .ok_or(EnvironmentFrameError::SkyStateInvalid)?;
    let global_sun = GlobalSunFrame {
        direction_world: sky.geocentric_sun_direction_world,
        linear_rgb: settings.sunlight.global_rgb,
        strength: settings.sunlight.global_strength,
    };
    let (halo_sources, halo_count, moon_orbit_world) = super::halo::prepare_halos(
        sky,
        render_view,
        disk_sun,
        disk_moon,
        moon_to_sun,
        &observed_bodies,
        settings,
    )?;
    let environment_uniform = EnvironmentUniform {
        sun_geometric_world: direction_altitude(
            sky.sun.geometric_direction_world,
            sky.sun.geometric_altitude_rad,
        )?,
        sun_apparent_world: direction_radius(
            sky.sun.apparent_direction_world,
            sky.sun.angular_radius_rad * SKY_DISK_DISPLAY_SCALE,
        )?,
        moon_geometric_world: direction_altitude(
            sky.moon.body.geometric_direction_world,
            sky.moon.body.geometric_altitude_rad,
        )?,
        moon_apparent_world: direction_radius(
            sky.moon.body.apparent_direction_world,
            sky.moon.body.angular_radius_rad * SKY_DISK_DISPLAY_SCALE,
        )?,
        moon_light_world_phase: direction_fraction(moon_to_sun, sky.moon.illuminated_fraction)?,
        moon_pole_world: direction_zero(moon_pole)?,
        local_sun_rgb_visibility: color_fraction(
            disk_sun.effective_linear_rgb,
            disk_sun.visible_fraction,
        )?,
        local_moon_rgb_visibility: color_fraction(
            disk_moon.effective_linear_rgb,
            disk_moon.visible_fraction,
        )?,
        global_sun_direction: direction_zero(sky.geocentric_sun_direction_world)?,
        global_sun_rgb: color_zero(
            settings
                .sunlight
                .global_rgb
                .map(|channel| channel * settings.sunlight.global_strength),
        )?,
        ambient_rgb_exposure: color_exposure(
            clear_sky.ambient_rgb,
            if render_view.kind == super::EnvironmentViewKind::Overview {
                // Orbit exposure belongs to the view, not the player's hemisphere.
                settings.exposure
            } else {
                clear_sky.exposure
            },
        )?,
        sky_zenith_rgb_sun_altitude: color_altitude(
            clear_sky.zenith_rgb,
            sky.sun.apparent_altitude_rad,
        )?,
        sky_horizon_rgb_star_visibility: color_visibility(
            clear_sky.horizon_rgb,
            clear_sky.star_visibility,
        )?,
        observer_up_horizon_cos: direction_fraction(observer_up, horizon_cos)?,
        flags: [
            descriptor.profile() as u32,
            render_view.kind as u32,
            1,
            transfer_route(render_view),
        ],
        eqj_to_world: matrix4(sky.catalog_epoch_to_world)?,
        moon_disk_radiance: super::super::moon_lighting::moon_disk_radiance(moon_flux),
        star_animation: [0.0; 4],
        weather_fog: [0.0; 4],
        moon_orbit_world,
        halo_count: [halo_count, 0, 0, 0],
        halo_sources,
    };
    Ok(EnvironmentFrame {
        context: Some(context),
        descriptor: Some(descriptor),
        clock_revision: Some(clock_revision),
        sky: Some(*sky),
        observed_bodies,
        halo_sources,
        halo_count,
        observer: Some(observer),
        render_view,
        global_sun: Some(global_sun),
        local_sun: Some(disk_sun),
        local_moon: Some(disk_moon),
        diagnostics: EnvironmentFrameDiagnostics {
            eye_below_planet_radius: below_radius,
            clear_sky_model_revision: super::super::lighting::CLEAR_SKY_MODEL_REVISION,
            star_frame: None,
        },
        status: EnvironmentFrameStatus::Available,
        camera_uniform: camera_uniform(render_view),
        environment_uniform,
    })
}

pub(super) fn interior(
    context: WorldContext,
    clock_revision: ClockRevision,
    descriptor: EnvironmentDescriptor,
    render_view: RenderViewFrame,
) -> Result<EnvironmentFrame, EnvironmentFrameError> {
    if descriptor.profile() != EnvironmentProfile::Interior {
        return Err(EnvironmentFrameError::DescriptorMismatch);
    }
    if descriptor.validate_clock(ClockId::MVP).is_err() || !render_view.is_finite() {
        return Err(EnvironmentFrameError::DescriptorMismatch);
    }
    let (observer_up, horizon_cos, below_radius) = view_horizon(render_view);
    let settings = EnvironmentSettings::interior();
    Ok(EnvironmentFrame {
        context: Some(context),
        descriptor: Some(descriptor),
        clock_revision: Some(clock_revision),
        sky: None,
        observed_bodies: Vec::new(),
        halo_sources: [super::HaloSource::zeroed(); super::MAX_HALO_BODIES],
        halo_count: 0,
        observer: None,
        render_view,
        global_sun: None,
        local_sun: None,
        local_moon: None,
        diagnostics: EnvironmentFrameDiagnostics {
            eye_below_planet_radius: below_radius,
            clear_sky_model_revision: 0,
            star_frame: None,
        },
        status: EnvironmentFrameStatus::Interior,
        camera_uniform: camera_uniform(render_view),
        environment_uniform: EnvironmentUniform {
            ambient_rgb_exposure: color_exposure(settings.ambient, settings.exposure)?,
            observer_up_horizon_cos: direction_fraction(observer_up, horizon_cos)?,
            flags: [
                EnvironmentProfile::Interior as u32,
                render_view.kind as u32,
                1,
                transfer_route(render_view),
            ],
            ..EnvironmentUniform::zeroed()
        },
    })
}

pub(super) fn unavailable(render_view: RenderViewFrame) -> EnvironmentFrame {
    EnvironmentFrame {
        context: None,
        descriptor: None,
        clock_revision: None,
        sky: None,
        observed_bodies: Vec::new(),
        halo_sources: [super::HaloSource::zeroed(); super::MAX_HALO_BODIES],
        halo_count: 0,
        observer: None,
        render_view,
        global_sun: None,
        local_sun: None,
        local_moon: None,
        diagnostics: EnvironmentFrameDiagnostics::default(),
        status: EnvironmentFrameStatus::Unavailable,
        camera_uniform: camera_uniform(render_view),
        environment_uniform: EnvironmentUniform {
            ambient_rgb_exposure: [0.08, 0.09, 0.12, 1.0],
            flags: [0, render_view.kind as u32, 0, transfer_route(render_view)],
            ..EnvironmentUniform::zeroed()
        },
    }
}

fn view_horizon(view: RenderViewFrame) -> ([f64; 3], f64, bool) {
    let radius = view.game_planet_radius_m;
    let eye = view.camera_eye_world_m;
    let eye_radius = length(eye);
    if radius <= 0.0 || !eye_radius.is_finite() || eye_radius <= 0.0 {
        return ([0.0, 1.0, 0.0], 0.0, false);
    }
    let up = eye.map(|value| value / eye_radius);
    if eye_radius < radius {
        return (up, 0.0, true);
    }
    let ratio = (radius / eye_radius).clamp(0.0, 1.0);
    let horizon_cos = -(1.0 - ratio * ratio).max(0.0).sqrt();
    (up, horizon_cos, false)
}

fn rotate_shortest_arc(from: [f64; 3], to: [f64; 3], vector: [f64; 3]) -> Option<[f64; 3]> {
    let from = normalize(from)?;
    let to = normalize(to)?;
    let vector = normalize(vector)?;
    let cosine = dot(from, to).clamp(-1.0, 1.0);
    if cosine < -1.0 + 1.0e-10 {
        let basis = if from[0].abs() < from[1].abs() && from[0].abs() < from[2].abs() {
            [1.0, 0.0, 0.0]
        } else if from[1].abs() < from[2].abs() {
            [0.0, 1.0, 0.0]
        } else {
            [0.0, 0.0, 1.0]
        };
        let axis = normalize(cross(from, basis))?;
        return normalize(add(
            scale(axis, 2.0 * dot(axis, vector)),
            scale(vector, -1.0),
        ));
    }
    let axis = cross(from, to);
    let first = cross(axis, vector);
    let second = cross(axis, first);
    normalize(add(add(vector, first), scale(second, 1.0 / (1.0 + cosine))))
}

fn valid_body(body: minimium_astronomy::CelestialBodyState) -> bool {
    valid_vector(body.geometric_direction_world)
        && valid_vector(body.apparent_direction_world)
        && body.geometric_altitude_rad.is_finite()
        && body.apparent_altitude_rad.is_finite()
        && body.distance_m.is_finite()
        && body.distance_m > 0.0
        && body.angular_radius_rad.is_finite()
        && (0.0..=PI / 2.0).contains(&body.angular_radius_rad)
}

fn direction_altitude(
    direction: [f64; 3],
    altitude: f64,
) -> Result<[f32; 4], EnvironmentFrameError> {
    let mut output = direction_zero(direction)?;
    output[3] = finite_f32(altitude)?;
    Ok(output)
}

fn direction_radius(direction: [f64; 3], radius: f64) -> Result<[f32; 4], EnvironmentFrameError> {
    let mut output = direction_zero(direction)?;
    output[3] = finite_f32(radius)?;
    Ok(output)
}

fn direction_zero(direction: [f64; 3]) -> Result<[f32; 4], EnvironmentFrameError> {
    let normalized = normalize(direction).ok_or(EnvironmentFrameError::SkyStateInvalid)?;
    Ok([
        finite_f32(normalized[0])?,
        finite_f32(normalized[1])?,
        finite_f32(normalized[2])?,
        0.0,
    ])
}

fn direction_fraction(
    direction: [f64; 3],
    fraction: f64,
) -> Result<[f32; 4], EnvironmentFrameError> {
    let mut output = direction_zero(direction)?;
    output[3] = finite_f32(fraction)?;
    Ok(output)
}

fn color_zero(color: [f32; 3]) -> Result<[f32; 4], EnvironmentFrameError> {
    if !color.iter().all(|value| value.is_finite() && *value >= 0.0) {
        return Err(EnvironmentFrameError::SettingsInvalid);
    }
    Ok([color[0], color[1], color[2], 0.0])
}

fn color_fraction(color: [f32; 3], fraction: f32) -> Result<[f32; 4], EnvironmentFrameError> {
    let mut output = color_zero(color)?;
    output[3] = fraction.clamp(0.0, 1.0);
    Ok(output)
}

fn color_exposure(color: [f32; 3], exposure: f32) -> Result<[f32; 4], EnvironmentFrameError> {
    if !exposure.is_finite() || exposure <= 0.0 {
        return Err(EnvironmentFrameError::SettingsInvalid);
    }
    let mut output = color_zero(color)?;
    output[3] = exposure;
    Ok(output)
}

fn color_altitude(color: [f32; 3], altitude: f64) -> Result<[f32; 4], EnvironmentFrameError> {
    let mut output = color_zero(color)?;
    output[3] = finite_f32(altitude)?;
    Ok(output)
}

fn color_visibility(color: [f32; 3], visibility: f32) -> Result<[f32; 4], EnvironmentFrameError> {
    if !visibility.is_finite() {
        return Err(EnvironmentFrameError::SkyStateInvalid);
    }
    let mut output = color_zero(color)?;
    output[3] = visibility.clamp(0.0, 1.0);
    Ok(output)
}

fn matrix4(matrix: [[f64; 3]; 3]) -> Result<[[f32; 4]; 4], EnvironmentFrameError> {
    if !matrix.iter().flatten().all(|value| value.is_finite()) {
        return Err(EnvironmentFrameError::SkyStateInvalid);
    }
    let columns: [[f64; 4]; 3] = std::array::from_fn(|column| {
        [matrix[0][column], matrix[1][column], matrix[2][column], 0.0]
    });
    let output = [columns[0], columns[1], columns[2], [0.0, 0.0, 0.0, 1.0]];
    output
        .iter()
        .flatten()
        .all(|value| value.is_finite() && value.abs() <= f32::MAX as f64)
        .then(|| output.map(|column| column.map(|value| value as f32)))
        .ok_or(EnvironmentFrameError::SkyStateInvalid)
}

fn transfer_route(view: RenderViewFrame) -> u32 {
    u32::from(!view.srgb_surface)
}

fn finite_f32(value: f64) -> Result<f32, EnvironmentFrameError> {
    if !value.is_finite() || value.abs() > f32::MAX as f64 {
        return Err(EnvironmentFrameError::SkyStateInvalid);
    }
    Ok(value as f32)
}

fn valid_vector(values: impl IntoIterator<Item = f64>) -> bool {
    values.into_iter().all(f64::is_finite)
}

fn normalize(vector: [f64; 3]) -> Option<[f64; 3]> {
    let length = length(vector);
    (length.is_finite() && length > 1.0e-15).then(|| vector.map(|value| value / length))
}

fn length(vector: [f64; 3]) -> f64 {
    vector.iter().map(|value| value * value).sum::<f64>().sqrt()
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|axis| a[axis] * b[axis]).sum()
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn scale(vector: [f64; 3], factor: f64) -> [f64; 3] {
    vector.map(|value| value * factor)
}
fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|axis| a[axis] + b[axis])
}
