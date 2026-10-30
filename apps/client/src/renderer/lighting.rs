use super::environment::EnvironmentSettings;
use minimium_astronomy::SkyState;
use minimium_core::sky::ObserverFrame;

pub(super) const CLEAR_SKY_MODEL_REVISION: u32 = 1;
const CLEAR_AIR_EXTINCTION: f64 = 0.08;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ClearSkyParameters {
    pub(super) ambient_rgb: [f32; 3],
    pub(super) exposure: f32,
    pub(super) zenith_rgb: [f32; 3],
    pub(super) horizon_rgb: [f32; 3],
    pub(super) star_visibility: f32,
    pub(super) direct_sun_factor: f32,
}

#[derive(Clone, Copy)]
struct Node {
    altitude_rad: f64,
    zenith_rgb: [f32; 3],
    horizon_rgb: [f32; 3],
    ambient_rgb: [f32; 3],
    exposure: f32,
    stars: f32,
    direct: f32,
}

// Relative linear RGB in unit-sun renderer space; values are appearance controls, not SI.
const NODES: [Node; 5] = [
    Node {
        altitude_rad: -18.0_f64.to_radians(),
        zenith_rgb: [0.0008, 0.0012, 0.0040],
        horizon_rgb: [0.0018, 0.0026, 0.0070],
        ambient_rgb: [0.0010, 0.0012, 0.0028],
        exposure: 1.25,
        stars: 1.0,
        direct: 0.0,
    },
    Node {
        altitude_rad: -12.0_f64.to_radians(),
        zenith_rgb: [0.0025, 0.0040, 0.0130],
        horizon_rgb: [0.0090, 0.0080, 0.0200],
        ambient_rgb: [0.0060, 0.0050, 0.0090],
        exposure: 1.15,
        stars: 0.5,
        direct: 0.0,
    },
    Node {
        altitude_rad: -6.0_f64.to_radians(),
        zenith_rgb: [0.0120, 0.0260, 0.0800],
        horizon_rgb: [0.2500, 0.0700, 0.0250],
        ambient_rgb: [0.0450, 0.0300, 0.0220],
        exposure: 1.05,
        stars: 0.0,
        direct: 0.0,
    },
    Node {
        altitude_rad: 0.0,
        zenith_rgb: [0.0750, 0.1500, 0.3200],
        horizon_rgb: [0.8500, 0.3000, 0.1000],
        ambient_rgb: [0.1200, 0.1050, 0.0900],
        exposure: 1.0,
        stars: 0.0,
        direct: 0.18,
    },
    Node {
        altitude_rad: 15.0_f64.to_radians(),
        zenith_rgb: [0.1500, 0.3600, 0.7800],
        horizon_rgb: [0.4800, 0.6900, 0.8800],
        ambient_rgb: [0.2300, 0.2650, 0.3000],
        exposure: 0.95,
        stars: 0.0,
        direct: 1.0,
    },
];

pub(super) fn evaluate_clear_sky_parameters(
    sky: &SkyState,
    observer: &ObserverFrame,
    settings: &EnvironmentSettings,
) -> ClearSkyParameters {
    let altitude = sky.sun.apparent_altitude_rad;
    let node = interpolate(altitude);
    let atmospheric_density = (-observer.geographic.height_m().max(0.0) / 8_500.0)
        .exp()
        .clamp(0.55, 1.0);
    let upper_limb_altitude = altitude + sky.sun.angular_radius_rad;
    let transmittance = if upper_limb_altitude <= -1.0_f64.to_radians() {
        0.0
    } else {
        direct_transmittance(upper_limb_altitude, atmospheric_density)
    };
    ClearSkyParameters {
        ambient_rgb: multiply(node.ambient_rgb, settings.ambient_rgb),
        exposure: node.exposure * settings.exposure,
        zenith_rgb: node.zenith_rgb,
        horizon_rgb: node.horizon_rgb,
        star_visibility: node.stars,
        direct_sun_factor: node.direct * transmittance,
    }
}

fn interpolate(altitude: f64) -> Node {
    let altitude = altitude.clamp(NODES[0].altitude_rad, NODES[4].altitude_rad);
    let upper = NODES
        .iter()
        .position(|node| node.altitude_rad >= altitude)
        .unwrap_or(NODES.len() - 1);
    if upper == 0 {
        return NODES[0];
    }
    let a = NODES[upper - 1];
    let b = NODES[upper];
    let linear = ((altitude - a.altitude_rad) / (b.altitude_rad - a.altitude_rad)).clamp(0.0, 1.0);
    let t = linear * linear * (3.0 - 2.0 * linear);
    Node {
        altitude_rad: altitude,
        zenith_rgb: mix(a.zenith_rgb, b.zenith_rgb, t),
        horizon_rgb: mix(a.horizon_rgb, b.horizon_rgb, t),
        ambient_rgb: mix(a.ambient_rgb, b.ambient_rgb, t),
        exposure: mix_scalar(a.exposure, b.exposure, t),
        stars: mix_scalar(a.stars, b.stars, t),
        direct: mix_scalar(a.direct, b.direct, t),
    }
}

fn direct_transmittance(altitude: f64, density: f64) -> f32 {
    let zenith_degrees = (90.0 - altitude.to_degrees()).clamp(0.0, 90.0);
    let correction = (96.07995 - zenith_degrees).max(1.0).powf(-1.6364) * 0.50572;
    let air_mass = (1.0 / (altitude.sin() + correction).max(0.025)).clamp(1.0, 40.0);
    (-CLEAR_AIR_EXTINCTION * air_mass * density).exp() as f32
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
