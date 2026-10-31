//! Semi-Lagrangian advection and bounded thermodynamic forcing on a sphere.
//! Intended for recognizable circulation/weather, not conservative NWP.
use super::grid::{coordinates, sample, AtmosphericCell, WeatherSurface, GRID_HEIGHT, GRID_WIDTH, LEVELS};
use super::{AtmosphereProfile, Condensable};
use std::f64::consts::{PI, TAU};

const STEP_SECONDS: f64 = 600.0;
const COHORT_SECONDS: i64 = 21_600;
const WARMUP_SECONDS: i64 = 10_800;
const LEVEL_HEIGHTS: [f64; LEVELS] = [800.0, 4500.0, 10500.0];

pub(crate) fn saturation(temperature_k: f64) -> f64 {
    (0.014 * ((temperature_k - 293.15) * 0.059).exp()).clamp(0.00002, 0.08)
}

fn random(seed: u128, x: usize, y: usize, level: usize) -> f64 {
    let mut key = (seed as u64 ^ (seed >> 64) as u64)
        ^ (x as u64).wrapping_mul(0x9e3779b97f4a7c15)
        ^ (y as u64).wrapping_mul(0xbf58476d1ce4e5b9)
        ^ (level as u64).wrapping_mul(0x94d049bb133111eb);
    key = (key ^ (key >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    key = (key ^ (key >> 27)).wrapping_mul(0x94d049bb133111eb);
    (key ^ (key >> 31)) as f64 / u64::MAX as f64
}

pub(crate) fn solar(latitude: f64, longitude: f64, seconds: f64, p: AtmosphereProfile) -> f64 {
    let orbit = (seconds / p.orbital_period_s).rem_euclid(1.0);
    let declination = p.obliquity_rad * (TAU * (orbit - 0.218)).sin();
    // The UTC epoch is an explicit approximation of an orbital phase anchor.
    // Solar, not sidereal, rotation: Earth sunrise must not drift by ~4 min/day.
    // The same spin rate still drives Coriolis independently.
    let solar_rotation = p.rotation_rad_s - TAU / p.orbital_period_s;
    let local_phase = (seconds * solar_rotation + longitude + PI).rem_euclid(TAU);
    (latitude.sin() * declination.sin()
        + latitude.cos() * declination.cos() * local_phase.cos()).max(0.0)
}

pub(crate) fn cohort(
    seed: u128,
    epoch: i64,
    utc_ms: i64,
    profile: AtmosphereProfile,
    surface: &WeatherSurface,
) -> Vec<AtmosphericCell> {
    let birth = epoch.saturating_mul(COHORT_SECONDS);
    let start = birth.saturating_sub(WARMUP_SECONDS);
    let target = utc_ms as f64 / 1000.0;
    let mut cells = initial(seed ^ (epoch as u128).wrapping_mul(719), start as f64, profile, surface);
    let mut time = start as f64;
    // Each cohort has at most 15 hours of integration. Clock jumps are bounded.
    while time < target {
        let dt = (target - time).min(STEP_SECONDS);
        if dt <= 0.0 { break; }
        cells = integrate(&cells, surface, time + dt, dt, profile);
        time += dt;
    }
    cells
}

fn initial(seed: u128, seconds: f64, p: AtmosphereProfile,
    surface: &WeatherSurface) -> Vec<AtmosphericCell> {
    let mut cells = vec![AtmosphericCell::default(); GRID_WIDTH * GRID_HEIGHT * LEVELS];
    for level in 0..LEVELS {
        for y in 0..GRID_HEIGHT {
            for x in 0..GRID_WIDTH {
                let (lat, lon) = coordinates(x, y);
                let ground = surface.at(x, y);
                let solar_energy = solar(lat, lon, seconds, p);
                let wave = (lon * 3.0 + lat * 5.0 + random(seed, x, y, level) * 1.2).sin();
                let height = LEVEL_HEIGHTS[level];
                let lapse = height * 0.006;
                let pole = lat.sin().abs();
                let water_response = 0.25 + ground.water * 0.75;
                let temperature = (p.surface_temperature_k + 13.0 * (1.0 - pole)
                    - 42.0 * pole + solar_energy * (8.0 - water_response * 5.0)
                    - lapse - ground.elevation_m * 0.004 + wave * 2.0).max(160.0);
                let moisture = saturation(temperature)
                    * (0.32 + ground.water * 0.32 + 0.12 * wave).clamp(0.08, 0.96);
                let pressure = p.surface_pressure_pa
                    * (-height / 8000.0).exp()
                    * (1.0 + wave * 0.006);
                let belt = if pole < 0.28 { -7.0 } else if pole < 0.78 { 18.0 } else { -5.0 };
                cells[(level * GRID_HEIGHT + y) * GRID_WIDTH + x] = AtmosphericCell {
                    temperature_k: temperature,
                    vapor_kg_kg: moisture,
                    condensate_kg_kg: ((wave + 0.3).max(0.0) * 0.0003).min(0.001),
                    pressure_pa: pressure,
                    east_m_s: belt * (1.0 + level as f64 * 0.7),
                    north_m_s: wave * 2.0,
                };
            }
        }
    }
    cells
}

fn integrate(old: &[AtmosphericCell], surface: &WeatherSurface, seconds: f64,
    dt: f64, p: AtmosphereProfile) -> Vec<AtmosphericCell> {
    let mut next = vec![AtmosphericCell::default(); old.len()];
    let dlat = PI / GRID_HEIGHT as f64;
    let dlon = TAU / GRID_WIDTH as f64;
    for layer in 0..LEVELS {
        for y in 0..GRID_HEIGHT {
            for x in 0..GRID_WIDTH {
                let (lat, lon) = coordinates(x, y);
                let index = (layer * GRID_HEIGHT + y) * GRID_WIDTH + x;
                let local = old[index];
                let ground = surface.at(x, y);
                let cos_lat = lat.cos().abs().max(0.15);
                let back_lat = (lat - local.north_m_s * dt / p.radius_m)
                    .clamp(-PI * 0.5 + 0.001, PI * 0.5 - 0.001);
                let back_lon = lon - local.east_m_s * dt / (p.radius_m * cos_lat);
                let mut air = sample(old, back_lat, back_lon, layer);
                let east_p = sample(old, lat, lon + dlon, layer).pressure_pa;
                let west_p = sample(old, lat, lon - dlon, layer).pressure_pa;
                let north_p = sample(old, lat + dlat, lon, layer).pressure_pa;
                let south_p = sample(old, lat - dlat, lon, layer).pressure_pa;
                let rho = (air.pressure_pa / (287.05 * air.temperature_k)).max(0.05);
                let coriolis = 2.0 * p.rotation_rad_s * lat.sin();
                let grad_e = (east_p - west_p) / (2.0 * p.radius_m * cos_lat * dlon);
                let grad_n = (north_p - south_p) / (2.0 * p.radius_m * dlat);
                let u = air.east_m_s + (-grad_e / rho + coriolis * air.north_m_s) * dt;
                let v = air.north_m_s + (-grad_n / rho - coriolis * air.east_m_s) * dt;
                let damping = (-dt / (2.0 + layer as f64) / 86_400.0).exp();
                air.east_m_s = (u * damping).clamp(-65.0, 65.0);
                air.north_m_s = (v * damping).clamp(-65.0, 65.0);
                let light = solar(lat, lon, seconds, p);
                let inertia = 1.0 + ground.water * 6.0;
                let radiative = (light * p.stellar_flux_w_m2 / 1361.0 - 0.27)
                    * dt / (3600.0 * inertia) * 0.6;
                air.temperature_k += radiative;
                let height = LEVEL_HEIGHTS[layer];
                let baseline = p.surface_temperature_k - 45.0 * lat.sin().abs()
                    - height * 0.006;
                air.temperature_k = (air.temperature_k
                    + (baseline - air.temperature_k) * (dt / 172_800.0))
                    .clamp(150.0, 350.0);
                let pressure_target = p.surface_pressure_pa * (-height / 8000.0).exp();
                air.pressure_pa = (air.pressure_pa
                    + (pressure_target - air.pressure_pa) * (dt / 259_200.0)
                    + (air.temperature_k - local.temperature_k) * 1.3)
                    .max(100.0);
                // Surface exchange and condensation are vertically parameterized.
                let evaporate = if layer == 0 && p.condensable == Condensable::Water {
                    ground.water * 0.00008 * (light * 0.7 + 0.3) * dt / 3600.0
                } else { 0.0 };
                air.vapor_kg_kg += evaporate;
                let saturation = saturation(air.temperature_k);
                let excess = (air.vapor_kg_kg - saturation * 0.86).max(0.0);
                let condensation = excess * (dt / 3600.0).min(0.3);
                air.vapor_kg_kg = (air.vapor_kg_kg - condensation).max(0.0);
                air.condensate_kg_kg += condensation;
                let evaporation = (saturation - air.vapor_kg_kg).max(0.0)
                    .min(air.condensate_kg_kg) * dt / 14_400.0;
                air.condensate_kg_kg -= evaporation;
                air.vapor_kg_kg += evaporation;
                let fallout_period = if layer == 2 { 43_200.0 } else { 10_800.0 };
                let fallout = air.condensate_kg_kg.max(0.0) * dt / fallout_period;
                air.condensate_kg_kg = (air.condensate_kg_kg - fallout).clamp(0.0, 0.01);
                air.vapor_kg_kg = air.vapor_kg_kg.clamp(0.0, 0.08);
                next[index] = air;
            }
        }
    }
    next
}
