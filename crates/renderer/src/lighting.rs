use crate::profiles::{ClearSkyNode, LightingProfile as EnvironmentSettings};
use minimium_astronomy::SkyState;
use minimium_environment::sky::ObserverFrame;

pub const CLEAR_SKY_MODEL_REVISION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClearSkyParameters {
    pub ambient_rgb: [f32; 3],
    pub exposure: f32,
    pub zenith_rgb: [f32; 3],
    pub horizon_rgb: [f32; 3],
    pub star_visibility: f32,
    pub direct_sun_factor: f32,
}

pub fn evaluate_clear_sky_parameters(
    sky: &SkyState,
    observer: &ObserverFrame,
    settings: &EnvironmentSettings,
) -> ClearSkyParameters {
    let altitude = sky.sun.apparent_altitude_rad;
    let node = interpolate(altitude, &settings.clear_sky.nodes);
    let upper_limb_altitude = altitude + sky.sun.angular_radius_rad;
    let transmittance = if upper_limb_altitude <= -1.0_f64.to_radians() {
        0.0
    } else {
        crate::moon_lighting::atmospheric_transmittance(
            upper_limb_altitude,
            observer.geographic.height_m(),
        )
        .unwrap_or(f32::NAN)
    };
    ClearSkyParameters {
        ambient_rgb: multiply(node.ambient_rgb, settings.ambient),
        exposure: node.exposure * settings.exposure,
        zenith_rgb: node.zenith_rgb,
        horizon_rgb: node.horizon_rgb,
        star_visibility: node.stars,
        direct_sun_factor: node.direct * transmittance,
    }
}

fn interpolate(altitude: f64, nodes: &[ClearSkyNode; 5]) -> ClearSkyNode {
    let altitude = altitude.clamp(nodes[0].altitude_rad, nodes[4].altitude_rad);
    let upper = nodes
        .iter()
        .position(|node| node.altitude_rad >= altitude)
        .unwrap_or(nodes.len() - 1);
    if upper == 0 {
        return nodes[0];
    }
    let a = nodes[upper - 1];
    let b = nodes[upper];
    let linear = ((altitude - a.altitude_rad) / (b.altitude_rad - a.altitude_rad)).clamp(0.0, 1.0);
    let t = linear * linear * (3.0 - 2.0 * linear);
    ClearSkyNode {
        altitude_rad: altitude,
        zenith_rgb: mix(a.zenith_rgb, b.zenith_rgb, t),
        horizon_rgb: mix(a.horizon_rgb, b.horizon_rgb, t),
        ambient_rgb: mix(a.ambient_rgb, b.ambient_rgb, t),
        exposure: mix_scalar(a.exposure, b.exposure, t),
        stars: mix_scalar(a.stars, b.stars, t),
        direct: mix_scalar(a.direct, b.direct, t),
    }
}

fn multiply(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] * b[i])
}

fn mix(a: [f32; 3], b: [f32; 3], t: f64) -> [f32; 3] {
    std::array::from_fn(|i| mix_scalar(a[i], b[i], t))
}

fn mix_scalar(a: f32, b: f32, t: f64) -> f32 {
    a + (b - a) * t as f32
}
