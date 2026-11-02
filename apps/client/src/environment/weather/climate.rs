use super::{random, rotate, Conditions, LayerConditions, SeasonalProfile, WeatherModel};
use glam::DVec3;

pub(super) fn sample(model: &WeatherModel, p: DVec3, utc: f64) -> Conditions {
    let (ground, ocean) = model.terrain(p);
    let lat = p.y.asin();
    let lon = p.z.atan2(p.x);
    let water = |d: DVec3| {
        let (_, ocean) = model.terrain(d);
        f64::from(
            ocean
                || (model.terrain.is_earth()
                    && crate::environment::lakes::contains(
                        d.y.asin().to_degrees(),
                        d.z.atan2(d.x).to_degrees(),
                    )),
        )
    };
    // Moisture is carried eastward from a broad, continuously sampled fetch.
    let east = DVec3::new(-p.z, 0.0, p.x).normalize_or_zero();
    let north = p.cross(east);
    let mut supply = water(p) * 0.25;
    for i in 1..=4 {
        let upstream = (p - east * (i as f64 * 0.006)).normalize();
        for side in [-1.0, 0.0, 1.0] {
            supply += water((upstream + north * side * 0.002).normalize()) * 0.0625;
        }
    }
    let upstream_ground = model.terrain((p - east * 0.004).normalize()).0;
    let uplift = ((ground - upstream_ground) / model.scale / 1800.0).clamp(-1.0, 1.0);
    let time = utc.rem_euclid(86400.0) / 86400.0 * std::f64::consts::TAU + lon;
    let baseline = match model.profile {
        SeasonalProfile::AnnualMean => 28.0 - 52.0 * lat.sin().powi(2),
    };
    let temperature = baseline - 0.0065 * ground.max(0.0) / model.scale
        + (if ocean { 1.5 } else { 5.0 }) * (time - std::f64::consts::PI * 1.25).cos();
    let moving = rotate(
        p,
        -utc.rem_euclid(8_000_000.0) * std::f64::consts::TAU / 8_000_000.0,
    );
    let seed = model.seed();
    let weather = noise(moving * 34.0, seed) * 0.65 + noise(moving * 73.0, seed ^ 19) * 0.35;
    let dry_belt = (-((lat.abs() - 0.5) / 0.16).powi(2)).exp();
    let humidity = (0.28 + 0.44 * supply + 0.4 * (weather - 0.5) + uplift.max(0.0) * 0.18
        - dry_belt * 0.17)
        .clamp(0.02, 0.99);
    let layers = std::array::from_fn(|layer| {
        let threshold = [0.38, 0.45, 0.5][layer];
        let thermal = if layer == 2 {
            (-temperature / 80.0).clamp(-0.2, 0.2)
        } else {
            ((temperature + 30.0) / 80.0).clamp(0.0, 0.1)
        };
        let coverage =
            ((humidity - threshold) * 2.8 + [0.0, 0.03, 0.08][layer] * weather + thermal)
                .clamp(0.0, 0.95);
        let base = match layer {
            0 => ground.max(0.0) + (500.0 + (1.0 - humidity) * 1500.0) * model.scale,
            1 => (2600.0 + (1.0 - humidity) * 1900.0) * model.scale,
            _ => (7000.0 + (temperature + 30.0).clamp(0.0, 60.0) * 45.0) * model.scale,
        };
        LayerConditions {
            coverage,
            base_m: base.max(ground + 300.0 * model.scale),
            thickness_m: [
                400.0 + 1300.0 * humidity,
                500.0 + 700.0 * humidity,
                250.0 + 400.0 * humidity,
            ][layer]
                * model.scale,
            max_base_m: match layer {
                0 => ground.max(0.0) + 2000.0 * model.scale,
                1 => 6000.0 * model.scale,
                _ => 11000.0 * model.scale,
            },
            ceiling_m: match layer {
                0 => ground.max(0.0) + 3500.0 * model.scale,
                1 => 7000.0 * model.scale,
                _ => 12000.0 * model.scale,
            },
        }
    });
    Conditions {
        temperature_c: temperature,
        humidity,
        water_supply: supply,
        uplift,
        layers,
    }
}

/// Trilinear smooth value noise in 3-D; there are no face or longitude seams.
pub(crate) fn noise(p: DVec3, seed: u64) -> f64 {
    let cell = p.floor();
    let t = p - cell;
    let t = t * t * (DVec3::splat(3.0) - t * 2.0);
    let mut value = 0.0;
    for z in 0..2 {
        for y in 0..2 {
            for x in 0..2 {
                let key = seed
                    ^ ((cell.x as i64 + x) as u64).wrapping_mul(73856093)
                    ^ ((cell.y as i64 + y) as u64).wrapping_mul(19349663)
                    ^ ((cell.z as i64 + z) as u64).wrapping_mul(83492791);
                value += random(key)
                    * (if x == 0 { 1.0 - t.x } else { t.x })
                    * (if y == 0 { 1.0 - t.y } else { t.y })
                    * (if z == 0 { 1.0 - t.z } else { t.z });
            }
        }
    }
    value
}
