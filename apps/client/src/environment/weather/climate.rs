//! A visual climate parameterization, not a numerical forecast or fluid solver.
use super::{dynamics, terrain::TerrainInfluence, wind};
use std::f64::consts::TAU;

pub(super) fn field(
    lat: f64,
    lon: f64,
    days: f64,
    seed: u128,
    terrain: TerrainInfluence,
) -> [f64; 12] {
    let [system, front, curl] = dynamics::fronts(lat, lon, days, seed);
    let season = -(days / 365.2425 * TAU).cos();
    let local_day = (days + lon / TAU).rem_euclid(1.0);
    let declination = 0.409 * ((days - 80.0) / 365.2425 * TAU).sin();
    let solar =
        lat.sin() * declination.sin() - lat.cos() * declination.cos() * (local_day * TAU).cos();
    let belt = (-(lat / 0.27).powi(2)).exp() * 0.12
        + (-((lat.abs() - 0.9) / 0.28).powi(2)).exp() * 0.12
        - (-((lat.abs() - 0.48) / 0.18).powi(2)).exp() * 0.12;
    let mean_temperature = 29.0 - lat.sin().abs() * 49.0 + season * lat.sin() * 12.0
        - terrain.altitude_m.min(8000.0) * 0.006;
    let temperature =
        (mean_temperature + solar * (4.0 - terrain.water * 3.5) + (front - 0.5) * 10.0)
            .clamp(-40.0, 40.0);
    // Thermal inertia: water has a smaller daily cycle and lags the annual cycle.
    let water_temperature = mean_temperature - season * lat.sin() * 3.0
        + (-(days / 365.2425 * TAU - 0.4).cos() - season) * lat.sin() * 7.0;
    let unfrozen = ((water_temperature + 1.0) / 4.0).clamp(0.0, 1.0);
    let contrast = water_temperature - temperature;
    let up = glam::DVec3::new(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin());
    let (east_axis, north_axis) = wind::basis(up);
    let state = wind::WindState::new(days * 86400.0, seed);
    let velocity = state.velocity(up, terrain.altitude_m)
        + state.breeze(
            up,
            [
                terrain.shores[1] - terrain.shores[0],
                terrain.shores[3] - terrain.shores[2],
            ],
            0.0,
        );
    let east = velocity.dot(east_axis);
    let north = velocity.dot(north_axis);
    let speed = east.hypot(north);
    let upstream = (terrain.shores[usize::from(east > 0.0)] * east.abs()
        + terrain.shores[2 + usize::from(north > 0.0)] * north.abs())
        / speed.max(1.0);
    let onshore = upstream * (1.0 - terrain.water);
    // Directional 5/20 km water samples approximate fetch. Growth is delayed
    // downwind by a residence time; cold water can instead suppress convection.
    let fetch_m = upstream * 25000.0;
    let residence = fetch_m / speed.max(1.0);
    let onset = wind::smooth(120.0, 900.0, residence);
    let band = 0.7
        + 0.3
            * dynamics::noise(
                (up * 420.0 + (east_axis * -north + north_axis * east) * 4.0).to_array(),
                seed as u32,
            );
    let lake_effect = (contrast / 13.0).clamp(0.0, 1.0) * upstream * unfrozen * onset * band;
    let uplift = (east * terrain.slope[0] + north * terrain.slope[1]).clamp(-0.3, 0.5);
    let humidity = (0.22
        + system * 0.60
        + (front - 0.5) * 0.34
        + belt
        + terrain.water * 0.07
        + onshore * 0.05
        + lake_effect * 0.12
        + uplift * 0.15)
        .clamp(0.05, 0.995);
    let stability = (-contrast / 8.0).clamp(0.0, 1.0) * terrain.water;
    let convection = ((solar.max(0.0) * lat.cos().abs() * (1.0 - stability)
        + onshore * solar.max(0.0) * 0.35
        + lake_effect * 0.55
        + uplift)
        * humidity)
        .clamp(0.0, 1.0);
    let low = ((humidity - 0.28) * 1.8 + convection * 0.15 - stability * 0.28).clamp(0.0, 0.99);
    let middle = ((system - 0.30) * 1.45 + (front - 0.5) * 0.45 + uplift).clamp(0.0, 0.96);
    let high = ((curl - 0.3) * 1.3 + middle * 0.28).clamp(0.0, 0.88);
    let storm = ((low - 0.60) * 2.5).max(0.0) * ((middle - 0.35) * 2.2).max(0.0);
    let rain =
        (storm.powf(1.6) * (10.0 + convection * 26.0) + lake_effect * low * 3.0).clamp(0.0, 32.0);
    let saturation = ((humidity - 0.72) / 0.27).clamp(0.0, 1.0);
    // Cooling follows actual solar elevation, including seasonal/polar day length.
    let night_cooling = ((0.12 - solar) / 0.35).clamp(0.0, 1.0);
    let calm = (1.0 - speed / 12.0).clamp(0.0, 1.0);
    let radiation =
        night_cooling * calm * (1.0 - low * 0.5 - middle * 0.3) * (0.3 + terrain.valley);
    let advection = (-contrast / 9.0).clamp(0.0, 1.0) * terrain.water * (speed / 6.0).min(1.0);
    let steam = (contrast / 16.0).clamp(0.0, 1.0) * terrain.water * unfrozen;
    let fog = (saturation * (radiation + advection * 0.7 + steam * 0.65) * (1.0 - rain / 45.0))
        .clamp(0.0, 1.0);
    [
        low,
        middle,
        high,
        rain,
        humidity,
        temperature,
        fog,
        storm.min(1.0),
        east.clamp(-32.0, 32.0),
        north.clamp(-32.0, 32.0),
        convection,
        terrain.altitude_m,
    ]
}
