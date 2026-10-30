use std::f64::consts::{PI, TAU};

use crate::{
    frames::{dot, matrix_vector, normalize},
    model::{MoonState, SkyError},
};
pub(crate) struct LunarGeometryInput {
    pub moon_body: crate::model::CelestialBodyState,
    pub moon_geocentric_eqj_m: [f64; 3],
    pub earth_heliocentric_eqj_m: [f64; 3],
    pub observer_geocentric_eqj_m: [f64; 3],
    pub moon_north_eqj: [f64; 3],
    pub ecliptic_phase_rad: f64,
    pub apparent_v_magnitude: f64,
    pub eqj_to_world: [[f64; 3]; 3],
}

pub(crate) fn lunar_geometry(input: LunarGeometryInput) -> Result<MoonState, SkyError> {
    if !input.ecliptic_phase_rad.is_finite()
        || !(0.0..TAU).contains(&input.ecliptic_phase_rad)
        || !input.apparent_v_magnitude.is_finite()
    {
        return Err(SkyError::InvalidUpstreamValue);
    }
    let moon_to_sun_eqj = normalize(std::array::from_fn(|axis| {
        -input.earth_heliocentric_eqj_m[axis] - input.moon_geocentric_eqj_m[axis]
    }))?;
    let moon_to_observer_eqj = normalize(std::array::from_fn(|axis| {
        input.observer_geocentric_eqj_m[axis] - input.moon_geocentric_eqj_m[axis]
    }))?;
    let moon_to_sun_world = normalize(matrix_vector(input.eqj_to_world, moon_to_sun_eqj))?;
    let moon_to_observer_world =
        normalize(matrix_vector(input.eqj_to_world, moon_to_observer_eqj))?;
    let pole_world = normalize(matrix_vector(
        input.eqj_to_world,
        normalize(input.moon_north_eqj)?,
    ))?;
    let cos_phase = dot(moon_to_sun_world, moon_to_observer_world).clamp(-1.0, 1.0);
    let phase_angle_rad = cos_phase.acos();
    let illuminated_fraction = ((1.0 + cos_phase) / 2.0).clamp(0.0, 1.0);
    if !phase_angle_rad.is_finite() || !illuminated_fraction.is_finite() {
        return Err(SkyError::NonFinite);
    }
    Ok(MoonState {
        body: input.moon_body,
        moon_to_sun_world,
        moon_to_observer_world,
        pole_world,
        phase_angle_rad,
        illuminated_fraction,
        ecliptic_phase_rad: input.ecliptic_phase_rad,
        waxing: input.ecliptic_phase_rad < PI,
        apparent_v_magnitude: input.apparent_v_magnitude,
    })
}
