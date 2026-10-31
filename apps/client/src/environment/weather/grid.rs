//! Canonical geographic regions. Sampling never depends on a client's camera position.
use super::{
    column,
    maps::Request,
    terrain::{self, TerrainInfluence},
};
use minimium_weather::{AtmosphereProfile, WeatherGrid, WeatherSurface};

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
    circulation: WeatherGrid,
}

impl Grid {
    pub(super) fn build(
        request: Request,
        days: f64,
        terrain: &[TerrainInfluence],
        width: usize,
        surface: &WeatherSurface,
    ) -> Self {
        let milliseconds = (days * 86_400_000.0).round() as i64;
        let circulation = WeatherGrid::reconstruct(
            request.seed, milliseconds, AtmosphereProfile::earth(), surface,
        );
        let columns = (0..width * (width / 2 + 1))
            .map(|i| {
                let (lat, lon) = coordinates(width, i % width, i / width);
                let ground = terrain.get(i).copied().unwrap_or_default();
                column::evaluate(lat, lon, days, request.seed, ground)
            })
            .collect();
        Self { width, columns, circulation }
    }

    /// Physical circulation is applied after terrain refinement so that winds
    /// and humidity are not overwritten by the old climatology adapter.
    pub(super) fn apply_circulation(&self, lat: f64, lon: f64, values: &mut Column) {
        let low = self.circulation.sample(lat, lon, 0);
        let middle = self.circulation.sample(lat, lon, 1);
        let high = self.circulation.sample(lat, lon, 2);
        let air = [low, middle, high];
        values[5] = (values[5] * 0.65
            + (low.temperature_k - 273.15).clamp(-45.0, 45.0) * 0.35).clamp(-40.0, 40.0);
        values[4] = (values[4] * 0.55 + low.humidity() * 0.45).clamp(0.05, 0.995);
        values[8] = (values[8] * 0.45 + low.east_m_s * 0.55).clamp(-32.0, 32.0);
        values[9] = (values[9] * 0.45 + low.north_m_s * 0.55).clamp(-32.0, 32.0);
        for (layer, cell) in air.into_iter().enumerate() {
            // Existing cloud population geometry still defines detailed shapes.
            // Condensate and humidity now modulate their regional atmosphere.
            let condensate = (cell.condensate_kg_kg * 180.0).clamp(0.0, 1.0);
            let humidity = cell.humidity();
            values[layer] = (values[layer] * 0.55
                + ((humidity - 0.48).max(0.0) * 1.15 + condensate * 0.45)
                    .clamp(0.0, 1.0) * 0.45).clamp(0.0, 1.0);
            let index = 14 + layer * 4;
            values[index] = (values[index] * 0.52 + condensate * 0.48).clamp(0.0, 1.0);
            if layer > 0 {
                let wind = 22 + layer * 2;
                values[wind] = (values[wind] * 0.45 + cell.east_m_s * 0.55).clamp(-48.0, 48.0);
                values[wind + 1] =
                    (values[wind + 1] * 0.45 + cell.north_m_s * 0.55).clamp(-48.0, 48.0);
            }
        }
        values[3] = (values[3] * 0.7 + low.precipitation_mm_h() * 0.3)
            .clamp(0.0, 32.0);
        let lapse = (low.temperature_k - middle.temperature_k) / 3700.0;
        values[10] = (values[10] * 0.7 + ((lapse - 0.003) * 75.0).clamp(0.0, 1.0) * 0.3)
            .clamp(0.0, 1.0);
    }

    pub(super) fn circulation_fingerprint(&self) -> u64 {
        self.circulation.fingerprint()
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
