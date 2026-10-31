//! Canonical geographic regions. Sampling never depends on a client's camera position.
use super::{
    column,
    maps::Request,
    terrain::{self, TerrainInfluence},
};

pub(crate) const CHANNELS: usize = 32;
pub(crate) const ATLAS_LAYERS: usize = CHANNELS;
pub(super) const SNAPSHOT_LAYERS: usize = CHANNELS / 2;
// The last two CPU-only values retain regional water/valley baselines for terrain refinement.
pub(super) type Column = [f64; CHANNELS + 2];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WeatherResolution(u16);

impl Default for WeatherResolution {
    fn default() -> Self {
        Self(256)
    }
}

impl WeatherResolution {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        let width = value.parse().ok()?;
        matches!(width, 64 | 128 | 256 | 512).then_some(Self(width))
    }

    pub(crate) fn width(self) -> usize {
        usize::from(self.0)
    }

    pub(crate) fn description(self) -> String {
        let width = self.width();
        format!(
            "weather resolution {width} | {} x {} regions ({} total)",
            width,
            width / 2,
            width * width / 2
        )
    }
}

pub(super) fn coordinates(width: usize, x: usize, y: usize) -> (f64, f64) {
    let height = width / 2;
    let lat = (0.5 - y as f64 / height as f64) * std::f64::consts::PI;
    // All longitudes share the same physical column at each pole.
    let lon = if y == 0 || y == height {
        0.0
    } else {
        (x as f64 / width as f64 - 0.5) * std::f64::consts::TAU
    };
    (lat, lon)
}

pub(super) fn terrain_grid(request: Request) -> Vec<TerrainInfluence> {
    let width = request.resolution.width();
    (0..width * (width / 2 + 1))
        .map(|i| {
            let (lat, lon) = coordinates(width, i % width, i / width);
            terrain::sample(request.terrain, lat, lon)
        })
        .collect()
}

pub(super) struct Grid {
    width: usize,
    columns: Vec<Column>,
}

impl Grid {
    pub(super) fn build(
        request: Request,
        days: f64,
        terrain: &[TerrainInfluence],
        width: usize,
    ) -> Self {
        let columns = (0..width * (width / 2 + 1))
            .map(|i| {
                let (lat, lon) = coordinates(width, i % width, i / width);
                let ground = terrain.get(i).copied().unwrap_or_default();
                column::evaluate(lat, lon, days, request.seed, ground)
            })
            .collect();
        Self { width, columns }
    }

    pub(super) fn sample(&self, lat: f64, lon: f64) -> Column {
        let height = self.width / 2;
        let x = ((lon / std::f64::consts::TAU + 0.5).rem_euclid(1.0)) * self.width as f64;
        let y = ((0.5 - lat / std::f64::consts::PI) * height as f64).clamp(0.0, height as f64);
        let x0 = x.floor() as usize % self.width;
        let x1 = (x0 + 1) % self.width;
        let y0 = y.floor() as usize;
        let y1 = (y0 + 1).min(height);
        let tx = x.fract();
        let ty = y.fract();
        std::array::from_fn(|c| {
            let a = self.columns[y0 * self.width + x0][c] * (1.0 - tx)
                + self.columns[y0 * self.width + x1][c] * tx;
            let b = self.columns[y1 * self.width + x0][c] * (1.0 - tx)
                + self.columns[y1 * self.width + x1][c] * tx;
            a * (1.0 - ty) + b * ty
        })
    }
}
