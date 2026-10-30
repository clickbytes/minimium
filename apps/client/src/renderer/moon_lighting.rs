const MOON_REFERENCE_MAGNITUDE: f64 = -12.74;
const CLEAR_AIR_EXTINCTION: f64 = 0.08;

/// Relative lunar flux compared to a full Moon at -12.74 apparent V magnitude.
pub(super) fn apparent_moon_flux(apparent_v_magnitude: f64) -> Option<f32> {
    if !apparent_v_magnitude.is_finite() {
        return None;
    }
    let exponent = -0.4 * (apparent_v_magnitude - MOON_REFERENCE_MAGNITUDE);
    let flux = 10.0_f64.powf(exponent);
    (flux.is_finite() && flux <= f32::MAX as f64).then_some(flux as f32)
}

pub(super) fn moon_disk_radiance(apparent_flux: f32) -> [f32; 4] {
    // Neutral linear lunar albedo; keep the normalized apparent flux in w.
    [0.8, 0.8, 0.8, apparent_flux]
}

/// Shared Kasten-Young-style transmission for direct sunlight and moonlight.
pub(super) fn atmospheric_transmittance(altitude_rad: f64, observer_height_m: f64) -> Option<f32> {
    if !altitude_rad.is_finite() || !observer_height_m.is_finite() {
        return None;
    }
    let density = (-observer_height_m.max(0.0) / 8_500.0)
        .exp()
        .clamp(0.55, 1.0);
    let zenith_degrees = (90.0 - altitude_rad.to_degrees()).clamp(0.0, 90.0);
    let correction = (96.07995 - zenith_degrees).max(1.0).powf(-1.6364) * 0.50572;
    let air_mass = (1.0 / (altitude_rad.sin() + correction).max(0.025)).clamp(1.0, 40.0);
    let transmittance = (-CLEAR_AIR_EXTINCTION * air_mass * density).exp();
    (transmittance.is_finite() && (0.0..=1.0).contains(&transmittance))
        .then_some(transmittance as f32)
}

pub(super) fn lunar_irradiance_at_observer(
    apparent_flux: f32,
    apparent_altitude_rad: f64,
    angular_radius_rad: f64,
    observer_height_m: f64,
    full_moon_strength: f32,
) -> Option<f32> {
    if !apparent_flux.is_finite()
        || apparent_flux < 0.0
        || !angular_radius_rad.is_finite()
        || angular_radius_rad < 0.0
        || !full_moon_strength.is_finite()
        || full_moon_strength < 0.0
    {
        return None;
    }
    let upper_limb_altitude = apparent_altitude_rad + angular_radius_rad;
    let transmission = atmospheric_transmittance(upper_limb_altitude, observer_height_m)?;
    let irradiance = full_moon_strength * apparent_flux * transmission;
    irradiance.is_finite().then_some(irradiance)
}
