//! Camera-independent, bounded weather history for the whole visible planet.
use glam::DVec3;
use minimium_environment::weather::{TerrainSampler, WeatherModel};

pub const WIDTH: u32 = 65;
pub const HEIGHT: u32 = 33;
pub const LAYERS: u32 = 4;
pub const BYTES: u64 = WIDTH as u64 * HEIGHT as u64 * LAYERS as u64 * 8;

pub struct SurfaceAtlas {
    pub start_utc: f64,
    pub pixels: Vec<[half::f16; 4]>,
}

impl SurfaceAtlas {
    pub fn build<T: TerrainSampler>(
        model: &WeatherModel<T>,
        utc: f64,
        cancelled: impl Fn() -> bool,
    ) -> Option<Self> {
        let start_utc = (utc / 3600.0).floor() * 3600.0;
        let cells = (WIDTH * HEIGHT) as usize;
        let mut pixels = vec![[half::f16::ZERO; 4]; cells * LAYERS as usize];
        for hour in 0..2 {
            let time = start_utc + hour as f64 * 3600.0;
            for y in 0..HEIGHT {
                if cancelled() {
                    return None;
                }
                let latitude = (0.5 - f64::from(y) / f64::from(HEIGHT - 1)) * std::f64::consts::PI;
                for x in 0..WIDTH {
                    let index = (y * WIDTH + x) as usize;
                    // Reuse seam/pole samples exactly; no longitude-dependent polar cap.
                    if x > 0 && (x == WIDTH - 1 || y == 0 || y == HEIGHT - 1) {
                        for layer in 0..2 {
                            pixels[(hour * 2 + layer) * cells + index] =
                                pixels[(hour * 2 + layer) * cells + (y * WIDTH) as usize];
                        }
                        continue;
                    }
                    let longitude =
                        (f64::from(x) / f64::from(WIDTH - 1) - 0.5) * std::f64::consts::TAU;
                    let direction = direction(latitude, longitude);
                    let [cover, climate] = sample(model, direction, time);
                    pixels[hour * 2 * cells + index] = cover;
                    pixels[(hour * 2 + 1) * cells + index] = climate;
                }
            }
        }
        Some(Self { start_utc, pixels })
    }
}

pub(crate) fn direction(latitude: f64, longitude: f64) -> DVec3 {
    DVec3::new(
        latitude.cos() * longitude.cos(),
        latitude.sin(),
        latitude.cos() * longitude.sin(),
    )
}

pub(crate) fn sample<T: TerrainSampler>(
    model: &WeatherModel<T>,
    direction: DVec3,
    time: f64,
) -> [[half::f16; 4]; 2] {
    let state = model.surface_state(direction, time);
    [
        [
            state.cover.vegetation,
            state.cover.snow,
            state.cover.wetness,
            state.climate.moisture as f32,
        ],
        [
            state.climate.temperature_c as f32,
            state.climate_elevation_m as f32,
            0.0,
            0.0,
        ],
    ]
    .map(|v| v.map(half::f16::from_f32))
}
