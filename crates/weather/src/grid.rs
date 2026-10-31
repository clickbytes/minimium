use super::{dynamics, AtmosphereProfile};
use std::f64::consts::{PI, TAU};

pub const GRID_WIDTH: usize = 48;
pub const GRID_HEIGHT: usize = 24;
pub const LEVELS: usize = 3;
const COHORT_SECONDS: i64 = 21_600;

#[derive(Clone, Copy, Debug, Default)]
pub struct AtmosphericCell {
    pub temperature_k: f64,
    pub vapor_kg_kg: f64,
    pub condensate_kg_kg: f64,
    pub pressure_pa: f64,
    pub east_m_s: f64,
    pub north_m_s: f64,
}

impl AtmosphericCell {
    pub fn humidity(self) -> f64 {
        (self.vapor_kg_kg / dynamics::saturation(self.temperature_k)).clamp(0.0, 1.0)
    }

    pub fn precipitation_mm_h(self) -> f64 {
        (self.condensate_kg_kg * 1200.0).max(0.0)
    }

    fn blend(self, other: Self, t: f64) -> Self {
        let mix = |a, b| a + (b - a) * t;
        Self {
            temperature_k: mix(self.temperature_k, other.temperature_k),
            vapor_kg_kg: mix(self.vapor_kg_kg, other.vapor_kg_kg),
            condensate_kg_kg: mix(self.condensate_kg_kg, other.condensate_kg_kg),
            pressure_pa: mix(self.pressure_pa, other.pressure_pa),
            east_m_s: mix(self.east_m_s, other.east_m_s),
            north_m_s: mix(self.north_m_s, other.north_m_s),
        }
    }
}

#[derive(Clone, Copy)]
pub struct SurfaceCell {
    pub water: f64,
    pub elevation_m: f64,
}

#[derive(Clone)]
pub struct WeatherSurface {
    cells: Vec<SurfaceCell>,
}

impl WeatherSurface {
    pub fn new(mut sample: impl FnMut(f64, f64) -> SurfaceCell) -> Self {
        let cells = (0..GRID_WIDTH * GRID_HEIGHT)
            .map(|i| {
                let (lat, lon) = coordinates(i % GRID_WIDTH, i / GRID_WIDTH);
                sample(lat, lon)
            })
            .collect();
        Self { cells }
    }

    pub fn earth() -> Self {
        Self::new(|lat, lon| {
            let direction = [lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin()];
            let elevation = minimium_procgen::try_earth_elevation_m(direction).unwrap_or(0.0);
            SurfaceCell {
                water: if elevation < 0.0 { 1.0 } else { 0.0 },
                elevation_m: elevation.max(0.0),
            }
        })
    }

    pub fn uniform(water: f64, elevation_m: f64) -> Self {
        Self::new(|_, _| SurfaceCell {
            water: water.clamp(0.0, 1.0),
            elevation_m: elevation_m.max(0.0),
        })
    }

    pub(crate) fn at(&self, x: usize, y: usize) -> SurfaceCell {
        self.cells[y * GRID_WIDTH + x]
    }
}

#[derive(Clone)]
pub struct WeatherGrid {
    cells: Vec<AtmosphericCell>,
    pub utc_ms: i64,
    pub seed: u128,
}

impl WeatherGrid {
    /// Reconstruct two overlapping finite-horizon physical simulations.
    /// Cohorts fade in/out over six world hours, maintaining continuity even
    /// when time_scale jumps without replaying years of world history.
    pub fn reconstruct(
        seed: u128,
        utc_ms: i64,
        profile: AtmosphereProfile,
        surface: &WeatherSurface,
    ) -> Self {
        let seconds = utc_ms.div_euclid(1000);
        let epoch = seconds.div_euclid(COHORT_SECONDS);
        let old = dynamics::cohort(seed, epoch - 1, utc_ms, profile, surface);
        let new = dynamics::cohort(seed, epoch, utc_ms, profile, surface);
        let t = (seconds.rem_euclid(COHORT_SECONDS) as f64
            + utc_ms.rem_euclid(1000) as f64 / 1000.0)
            / COHORT_SECONDS as f64;
        let cells = old.into_iter().zip(new)
            .map(|(a, b)| a.blend(b, t))
            .collect();
        Self { cells, utc_ms, seed }
    }

    /// Quantized, endianness-independent comparison aid for client/server epochs.
    /// Not a cryptographic identity or cross-platform bitwise determinism proof.
    pub fn fingerprint(&self) -> u64 {
        let mut hash = 0xcbf29ce484222325u64;
        for cell in &self.cells {
            for (value, scale) in [
                (cell.temperature_k, 100.0),
                (cell.vapor_kg_kg, 10_000_000.0),
                (cell.condensate_kg_kg, 10_000_000.0),
                (cell.pressure_pa, 10.0),
                (cell.east_m_s, 100.0),
                (cell.north_m_s, 100.0),
            ] {
                hash = (hash ^ ((value * scale).round() as i64 as u64))
                    .wrapping_mul(0x100000001b3);
            }
        }
        hash
    }

    pub fn sample(&self, latitude_rad: f64, longitude_rad: f64, level: usize) -> AtmosphericCell {
        sample(&self.cells, latitude_rad, longitude_rad, level)
    }
}

pub(crate) fn coordinates(x: usize, y: usize) -> (f64, f64) {
    (
        (0.5 - (y as f64 + 0.5) / GRID_HEIGHT as f64) * PI,
        ((x as f64 + 0.5) / GRID_WIDTH as f64 - 0.5) * TAU,
    )
}

pub(crate) fn sample(
    cells: &[AtmosphericCell],
    lat: f64,
    lon: f64,
    level: usize,
) -> AtmosphericCell {
    let x = ((lon / TAU + 0.5).rem_euclid(1.0) * GRID_WIDTH as f64 - 0.5)
        .rem_euclid(GRID_WIDTH as f64);
    let y = ((0.5 - lat.clamp(-PI * 0.5, PI * 0.5) / PI) * GRID_HEIGHT as f64 - 0.5)
        .clamp(0.0, GRID_HEIGHT as f64 - 1.0);
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let ix = |x: usize, y: usize| {
        cells[(level.min(LEVELS - 1) * GRID_HEIGHT + y) * GRID_WIDTH + x % GRID_WIDTH]
    };
    let a = ix(x0, y0).blend(ix(x0 + 1, y0), x.fract());
    let b = ix(x0, (y0 + 1).min(GRID_HEIGHT - 1))
        .blend(ix(x0 + 1, (y0 + 1).min(GRID_HEIGHT - 1)), x.fract());
    a.blend(b, y.fract())
}
