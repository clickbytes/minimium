//! Shared GEBCO elevation and real lake polygons, independent of the camera direction.
use super::lakes;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TerrainConfig {
    pub(crate) height_scale_permille: u32,
    pub(crate) sea_level_mm: i64,
    pub(crate) radius_m: f64,
}

#[derive(Clone, Copy, Default)]
pub(super) struct TerrainInfluence {
    pub(super) water: f64,
    pub(super) valley: f64,
    pub(super) altitude_m: f64,
    /// Water fraction to the east, west, north, south within 5–20 km.
    pub(super) shores: [f64; 4],
    pub(super) slope: [f64; 2],
}

fn direction(latitude: f64, longitude: f64) -> [f64; 3] {
    [
        latitude.cos() * longitude.cos(),
        latitude.sin(),
        latitude.cos() * longitude.sin(),
    ]
}

pub(super) fn map(
    config: Option<TerrainConfig>,
    region: super::region::Region,
) -> Vec<TerrainInfluence> {
    use super::simulated::{HEIGHT, WIDTH};
    (0..WIDTH * HEIGHT)
        .map(|i| {
            let (lat, lon) = region.coordinates(i % WIDTH, i / WIDTH);
            sample(config, lat, lon)
        })
        .collect()
}

pub(super) fn sample(config: Option<TerrainConfig>, lat: f64, lon: f64) -> TerrainInfluence {
    let config = config.unwrap_or(TerrainConfig {
        height_scale_permille: 1000,
        sea_level_mm: 0,
        radius_m: 6_371_000.0,
    });
    let physical_scale = 6_371_000.0 / config.radius_m.max(1000.0);
    let elevation = |lat, lon| {
        minimium_procgen::try_earth_elevation_m(direction(lat, lon))
            .ok()
            .map(|m| {
                (m * f64::from(config.height_scale_permille) / 1000.0
                    - config.sea_level_mm as f64 / 1000.0)
                    * physical_scale
            })
    };
    let sample = |lat: f64, lon: f64| {
        let height = elevation(lat, lon);
        let water =
            height.is_some_and(|h| h < 0.0) || lakes::contains(lat.to_degrees(), lon.to_degrees());
        (height.unwrap_or(0.0).max(0.0), f64::from(water))
    };
    let (height, water) = sample(lat, lon);
    let mut shores = [0.0; 4];
    let mut heights = [0.0; 4];
    // Great-circle offsets remain well behaved across the date line and poles.
    for (i, bearing) in [90.0_f64, 270.0, 0.0, 180.0].into_iter().enumerate() {
        for distance in [5000.0, 20000.0] {
            let arc = distance / 6_371_000.0_f64;
            let bearing = bearing.to_radians();
            let next_lat = (lat.sin() * arc.cos() + lat.cos() * arc.sin() * bearing.cos()).asin();
            let next_lon = lon
                + (bearing.sin() * arc.sin() * lat.cos())
                    .atan2(arc.cos() - lat.sin() * next_lat.sin());
            let (h, w) = sample(next_lat, next_lon);
            shores[i] += w * 0.5;
            heights[i] += h * 0.5;
        }
    }
    TerrainInfluence {
        water,
        valley: ((heights.iter().sum::<f64>() * 0.25 - height) / 220.0).clamp(0.0, 1.0),
        altitude_m: height,
        shores,
        slope: [
            (heights[0] - heights[1]) / 25000.0,
            (heights[2] - heights[3]) / 25000.0,
        ],
    }
}
