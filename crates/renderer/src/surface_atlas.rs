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
    ) -> Result<Option<Self>, T::Error> {
        let mut builder = SurfaceAtlasBuilder::new(utc);
        while !cancelled() {
            if builder.build_row(model)? {
                return Ok(Some(builder.atlas));
            }
        }
        Ok(None)
    }
}

/// A weather worker can service cloud requests between rows of the hourly atlas.
pub struct SurfaceAtlasBuilder {
    atlas: SurfaceAtlas,
    row: u32,
}

impl SurfaceAtlasBuilder {
    pub fn new(utc: f64) -> Self {
        Self {
            atlas: SurfaceAtlas {
                start_utc: (utc / 3600.0).floor() * 3600.0,
                pixels: vec![[half::f16::ZERO; 4]; (WIDTH * HEIGHT * LAYERS) as usize],
            },
            row: 0,
        }
    }

    /// Returns true once both hourly endpoints are complete. A failed row can be retried.
    pub fn build_row<T: TerrainSampler>(
        &mut self,
        model: &WeatherModel<T>,
    ) -> Result<bool, T::Error> {
        if self.row == HEIGHT * 2 {
            return Ok(true);
        }
        let hour = (self.row / HEIGHT) as usize;
        let y = self.row % HEIGHT;
        let cells = (WIDTH * HEIGHT) as usize;
        let time = self.atlas.start_utc + hour as f64 * 3600.0;
        let latitude = (0.5 - f64::from(y) / f64::from(HEIGHT - 1)) * std::f64::consts::PI;
        for x in 0..WIDTH {
            let index = (y * WIDTH + x) as usize;
            // Reuse seam/pole samples exactly; no longitude-dependent polar cap.
            if x > 0 && (x == WIDTH - 1 || y == 0 || y == HEIGHT - 1) {
                for layer in 0..2 {
                    self.atlas.pixels[(hour * 2 + layer) * cells + index] =
                        self.atlas.pixels[(hour * 2 + layer) * cells + (y * WIDTH) as usize];
                }
                continue;
            }
            let longitude = (f64::from(x) / f64::from(WIDTH - 1) - 0.5) * std::f64::consts::TAU;
            let direction = direction(latitude, longitude);
            let [cover, climate] = sample(model, direction, time)?;
            self.atlas.pixels[hour * 2 * cells + index] = cover;
            self.atlas.pixels[(hour * 2 + 1) * cells + index] = climate;
        }
        self.row += 1;
        Ok(self.row == HEIGHT * 2)
    }

    pub fn finish(self) -> Option<SurfaceAtlas> {
        (self.row == HEIGHT * 2).then_some(self.atlas)
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
) -> Result<[[half::f16; 4]; 2], T::Error> {
    let state = model.surface_state(direction, time)?;
    Ok([
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
    .map(|v| v.map(half::f16::from_f32)))
}
