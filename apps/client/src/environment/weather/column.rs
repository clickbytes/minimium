//! Reconstructible atmospheric columns: coverage, condensate, depth and wind are separate.
//! This bounded visual parameterization deliberately needs no frame-by-frame history.
use super::{
    climate,
    grid::{Column, CHANNELS},
    patterns,
    terrain::TerrainInfluence,
    transport,
    wind::{self, WindState},
};
use glam::DVec3;

pub(super) fn evaluate(
    lat: f64,
    lon: f64,
    days: f64,
    seed: u128,
    terrain: TerrainInfluence,
) -> Column {
    let mut result = at_time(lat, lon, days, seed, terrain);
    let recent = at_time(lat, lon, days - 1.0 / 24.0, seed, terrain)[3];
    let older = at_time(lat, lon, days - 3.0 / 24.0, seed, terrain)[3];
    result[7] = (result[3] / 12.0 + recent / 18.0 + older / 30.0).clamp(0.0, 1.0);
    result[6] = (result[6] * (0.65 + result[7] * 0.55)).clamp(0.0, 1.0);
    result
}

fn at_time(lat: f64, lon: f64, days: f64, seed: u128, terrain: TerrainInfluence) -> Column {
    let mut result = [0.0; CHANNELS + 2];
    let state = climate::field(lat, lon, days, seed, terrain);
    result[..12].copy_from_slice(&state);
    let up = DVec3::new(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin());
    let flow = transport::frame(days * 86400.0, seed);
    let gamma = state[4].max(0.05).ln() + 17.625 * state[5] / (243.04 + state[5]);
    let dewpoint = 243.04 * gamma / (17.625 - gamma);
    let base = terrain.altitude_m + (125.0 * (state[5] - dewpoint)).clamp(180.0, 3200.0);
    let convection = state[10];
    let stability = (1.0 - convection) * (0.6 + terrain.water * 0.4);
    let mut water_path = 0.0;
    for layer in 0..3 {
        let growth = patterns::weather_pattern(
            &flow,
            up,
            [1800.0, 5500.0, 11000.0][layer],
            32.0 + layer as f64 * 9.0,
            (layer as u32 + 1).wrapping_mul(0x9e3779b9),
        );
        let supply = (0.18 + state[layer] * 0.45 + growth * 0.37).clamp(0.0, 1.0);
        let kind = if layer == 0 { convection } else { growth };
        let bottom = match layer {
            0 => base,
            1 => (4300.0 - lat.sin().abs() * 1700.0).max(base + 900.0),
            _ => (11000.0 - lat.sin().abs() * 4000.0).max(base + 3400.0),
        };
        let thickness = match layer {
            0 => {
                350.0
                    + convection * supply * 4200.0
                    + convection.powi(2) * (growth - 0.45).max(0.0) * 15000.0
            }
            1 => 650.0 + (1.0 - stability) * 1300.0 + growth * 1100.0,
            _ => 400.0 + growth * 1200.0,
        };
        let condensate = match layer {
            0 => supply * (0.35 + convection * 0.65),
            1 => supply * (0.25 + growth * 0.45),
            _ => supply * (0.12 + growth * 0.30),
        };
        let top = (bottom + thickness).min(17500.0);
        result[12 + layer * 4..16 + layer * 4].copy_from_slice(&[
            bottom.min(17400.0),
            top,
            condensate,
            kind,
        ]);
        result[layer] = (state[layer] * (0.65 + growth * 0.55)).clamp(0.0, 1.0);
        if layer < 2 {
            water_path += condensate * (top - bottom).max(0.0) * result[layer];
        }
    }
    // Rain grows with available condensate/depth; it no longer creates the storm tower.
    let conversion = ((water_path - 400.0) / 1800.0).clamp(0.0, 1.0);
    result[3] = (conversion.powf(1.5) * (3.0 + convection * 29.0) + state[3] * 0.15 * conversion)
        .clamp(0.0, 32.0);
    set_wind(&mut result, terrain, flow.wind, up);
    result[CHANNELS] = terrain.water;
    result[CHANNELS + 1] = terrain.valley;
    result
}

/// Continuous subregion terrain response. Both atlas views sample this same function;
/// changing the observer only changes where it is sampled more finely.
pub(super) fn refine(values: &mut Column, ground: TerrainInfluence, state: WindState, up: DVec3) {
    let old_ground = values[11];
    let water_delta = ground.water - values[CHANNELS];
    let shore = [
        ground.shores[1] - ground.shores[0],
        ground.shores[3] - ground.shores[2],
    ];
    let (east, north) = wind::basis(up);
    let surface_wind = state.velocity(up, ground.altitude_m) + state.breeze(up, shore, 0.0);
    values[8] = surface_wind.dot(east);
    values[9] = surface_wind.dot(north);
    let uplift = (values[8] * ground.slope[0] + values[9] * ground.slope[1]).clamp(-0.3, 0.5);
    let old_humidity = values[4];
    values[4] = (values[4] + water_delta * 0.07 + uplift * 0.08).clamp(0.05, 0.995);
    values[5] = (values[5] - (ground.altitude_m - old_ground) * 0.006).clamp(-40.0, 40.0);
    values[0] = (values[0] + (values[4] - old_humidity) * 1.8 + uplift * 0.1).clamp(0.0, 1.0);
    values[6] = (values[6] * (0.5 + ground.valley) / (0.5 + values[CHANNELS + 1])
        * (1.0 + water_delta * 0.4))
        .clamp(0.0, 1.0);
    let gamma = values[4].ln() + 17.625 * values[5] / (243.04 + values[5]);
    let dewpoint = 243.04 * gamma / (17.625 - gamma);
    let base =
        (ground.altitude_m + (125.0 * (values[5] - dewpoint)).clamp(180.0, 3200.0)).min(17400.0);
    let shift = base - values[12];
    values[12] = base;
    values[13] = (values[13] + shift + uplift * 900.0).clamp(base + 100.0, 17500.0);
    for layer in 1..3 {
        let bottom = values[12 + layer * 4]
            .max(base + layer as f64 * 800.0)
            .min(17400.0);
        values[12 + layer * 4] = bottom;
        values[13 + layer * 4] = values[13 + layer * 4].max(bottom + 100.0).min(17500.0);
    }
    values[3] = (values[3] * (1.0 + water_delta * 0.3 + uplift)).clamp(0.0, 32.0);
    values[11] = ground.altitude_m;
    set_wind(values, ground, state, up);
}

fn set_wind(values: &mut Column, ground: TerrainInfluence, state: WindState, up: DVec3) {
    let shore = [
        ground.shores[1] - ground.shores[0],
        ground.shores[3] - ground.shores[2],
    ];
    let (east, north) = wind::basis(up);
    for level in 0..3 {
        let height = if level == 0 {
            ground.altitude_m
        } else {
            (values[12 + level * 4] + values[13 + level * 4]) * 0.5
        };
        let velocity =
            state.velocity(up, height) + state.breeze(up, shore, height - ground.altitude_m);
        let index = if level == 0 { 8 } else { 22 + level * 2 };
        values[index] = velocity.dot(east);
        values[index + 1] = velocity.dot(north);
    }
    values[28..32].copy_from_slice(&[shore[0], shore[1], ground.slope[0], ground.slope[1]]);
}

pub(super) fn preset(sample: super::WeatherSample) -> Column {
    // Preset changes run on the UI thread; never construct trajectory maps here.
    let mut values = [0.0; CHANNELS + 2];
    values[..12].copy_from_slice(&[
        sample.low as f64,
        sample.middle as f64,
        sample.high as f64,
        sample.rain_mm_h as f64,
        sample.humidity as f64,
        sample.temperature_c as f64,
        sample.fog as f64,
        sample.ground_moisture as f64,
        sample.wind_east_m_s as f64,
        sample.wind_north_m_s as f64,
        0.35,
        0.0,
    ]);
    for layer in 0..3 {
        let base = [1300.0, 4300.0, 10000.0][layer];
        values[12 + layer * 4] = base;
        values[13 + layer * 4] = base + [1800.0, 1600.0, 900.0][layer];
        values[14 + layer * 4] = 0.25 + (sample.rain_mm_h as f64 / 12.0).min(0.5);
        values[15 + layer * 4] = [0.55, 0.35, 0.1][layer];
    }
    for layer in 0..2 {
        values[24 + layer * 2] = f64::from(sample.wind_east_m_s) * (1.3 + layer as f64 * 0.4);
        values[25 + layer * 2] = f64::from(sample.wind_north_m_s) * (1.3 + layer as f64 * 0.4);
    }
    values
}
