//! CPU grid published by a host worker and uploaded without history work in a frame.
use glam::DVec3;
use minimium_environment::weather::{TerrainSampler, WeatherModel};

pub const EDGE: u32 = 9;
pub const EXTENT: f64 = 0.01;

pub struct CoverGrid {
    pub start_utc: f64,
    pub region: [f32; 4],
    pub atlas: Option<std::sync::Arc<crate::surface_atlas::SurfaceAtlas>>,
    pub center: [f32; 4],
    pub east: [f32; 4],
    pub north: [f32; 4],
    pub pixels: Vec<[half::f16; 4]>,
    pub atmosphere_weather: [[f32; 4]; 2],
    pub fog_pixels: Vec<[half::f16; 4]>,
}

impl CoverGrid {
    pub fn build<T: TerrainSampler>(model: &WeatherModel<T>, observer: DVec3, utc: f64) -> Self {
        let center = (observer * 1000.0).round().normalize();
        let east = if center.y.abs() < 0.99 {
            DVec3::Y.cross(center).normalize()
        } else {
            DVec3::X.cross(center).normalize()
        };
        let north = center.cross(east);
        let start_utc = (utc / 3600.0).floor() * 3600.0;
        // Fixed angular lattice: overlapping windows evaluate identical sites.
        let step = EXTENT * 2.0 / f64::from(EDGE - 1);
        let latitude = observer.y.asin();
        let longitude = observer.z.atan2(observer.x);
        let start_lat = (latitude / step).floor() * step - EXTENT;
        let start_lon = (longitude / step).floor() * step - EXTENT;
        let cells = (EDGE * EDGE) as usize;
        let mut pixels = vec![[half::f16::ZERO; 4]; cells * 4];
        let mut atmosphere_weather = [[0.0; 4]; 2];
        let mut fog_pixels = Vec::with_capacity((EDGE * EDGE * 2) as usize);
        for hour in 0..2 {
            let time = start_utc + f64::from(hour) * 3600.0;
            for y in 0..EDGE {
                for x in 0..EDGE {
                    let u = (f64::from(x) / f64::from(EDGE - 1) * 2.0 - 1.0) * EXTENT;
                    let v = (f64::from(y) / f64::from(EDGE - 1) * 2.0 - 1.0) * EXTENT;
                    let direction = (center + east * u + north * v).normalize();
                    let sample = model.surface_cover(direction, time);
                    let conditions = model.sample(direction, time);
                    let moisture = conditions.humidity as f32;
                    let ground = model.atmospheric_ground_height_m(direction) as f32;
                    let humid = ((moisture - 0.65) / 0.3).clamp(0.0, 1.0);
                    let cool =
                        (1.0 - (conditions.temperature_c as f32 - 4.0) / 18.0).clamp(0.0, 1.0);
                    fog_pixels.push(
                        [
                            sample.wetness * humid * cool * 0.002,
                            100.0,
                            ground,
                            humid * 1e-5,
                        ]
                        .map(half::f16::from_f32),
                    );
                    if x == EDGE / 2 && y == EDGE / 2 {
                        atmosphere_weather[hour as usize] = [
                            moisture,
                            conditions.temperature_c as f32,
                            sample.wetness,
                            ground,
                        ];
                    }
                    let site = crate::surface_atlas::direction(
                        start_lat + f64::from(y) * step,
                        start_lon + f64::from(x) * step,
                    );
                    let [cover, climate] = crate::surface_atlas::sample(model, site, time);
                    let index = (y * EDGE + x) as usize;
                    pixels[hour as usize * 2 * cells + index] = cover;
                    pixels[(hour as usize * 2 + 1) * cells + index] = climate;
                }
            }
        }
        Self {
            start_utc,
            region: [
                start_lon as f32,
                start_lat as f32,
                step as f32,
                (EXTENT * 2.0) as f32,
            ],
            atlas: None,
            center: center.as_vec3().extend(EXTENT as f32).to_array(),
            east: east.as_vec3().extend(0.0).to_array(),
            north: north.as_vec3().extend(0.0).to_array(),
            pixels,
            atmosphere_weather,
            fog_pixels,
        }
    }
}
