use super::{adapter::EnvironmentFrameError, RenderViewFrame};
use minimium_astronomy::{SkyState, MOON_MEAN_RADIUS_M};

const EARTH_REFERENCE_RADIUS_M: f64 = 6_371_000.0;

/// Scale both the Moon's orbital position and physical radius by the same
/// Earth-to-game radius ratio, preserving their angular relationship.
pub(super) fn moon_orbit_world(
    sky: &SkyState,
    view: RenderViewFrame,
) -> Result<[f32; 4], EnvironmentFrameError> {
    let scale = view.game_planet_radius_m / EARTH_REFERENCE_RADIUS_M;
    let position = sky.geocentric_moon_position_world_m;
    let orbit = [
        (position[0] * scale) as f32,
        (position[1] * scale) as f32,
        (position[2] * scale) as f32,
        (MOON_MEAN_RADIUS_M * scale) as f32,
    ];
    if !orbit.iter().all(|value| value.is_finite()) || orbit[3] <= 0.0 {
        return Err(EnvironmentFrameError::SkyStateInvalid);
    }
    Ok(orbit)
}
