//! Bounded, monotone vertical relief curve in source metres above sea level.
//! This is part of a planet definition, shared by terrain and its consumers.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PeakRelief {
    pub start_m: u32,
    pub full_m: u32,
    pub multiplier_permille: u32,
    pub exponent: u32,
}

impl Default for PeakRelief {
    fn default() -> Self {
        Self::NONE
    }
}

impl PeakRelief {
    pub const NONE: Self = Self {
        start_m: 1000,
        full_m: 9000,
        multiplier_permille: 1000,
        exponent: 3,
    };

    pub const fn enabled(self) -> bool {
        self.multiplier_permille > 1000
    }

    pub const fn is_valid(self) -> bool {
        self.start_m < self.full_m
            && self.full_m <= 12_000
            && self.multiplier_permille >= 1000
            && self.multiplier_permille <= 3000
            && self.exponent >= 1
            && self.exponent <= 8
    }

    /// C1 at both endpoints. Below the threshold (including the seabed) is identity.
    /// Above full_m the multiplier stays bounded rather than growing exponentially.
    pub fn apply(self, height_m: f64) -> f64 {
        if !self.enabled() || height_m <= f64::from(self.start_m) {
            return height_m;
        }
        let t = ((height_m - f64::from(self.start_m)) / f64::from(self.full_m - self.start_m))
            .clamp(0.0, 1.0);
        let smooth = t * t * (3.0 - 2.0 * t);
        let mut power = 1.0;
        for _ in 0..self.exponent {
            power *= smooth;
        }
        height_m * (1.0 + (f64::from(self.multiplier_permille) / 1000.0 - 1.0) * power)
    }

    /// Fallback for consumers without source geography. Terrain adapters should
    /// expose the source height directly, preserving detail and clipping semantics.
    pub fn inverse(self, height_m: f64) -> f64 {
        if !self.enabled() || height_m <= f64::from(self.start_m) {
            return height_m;
        }
        let mut low = f64::from(self.start_m);
        let mut high = height_m;
        for _ in 0..32 {
            let middle = (low + high) * 0.5;
            if self.apply(middle) < height_m {
                low = middle;
            } else {
                high = middle;
            }
        }
        (low + high) * 0.5
    }
}

impl super::PlanetDefinition {
    /// Unclipped generated elevation above the planet datum, in millimetres.
    pub fn earth_elevation_mm(self, source_m: f64) -> f64 {
        let scale = f64::from(self.height_scale_permille);
        // Preserve the original arithmetic exactly for revisions 3/4.
        if !self.peak_relief.enabled() {
            return source_m * scale;
        }
        let sea = self.sea_level_mm as f64;
        let relative = source_m - sea / scale;
        if relative <= f64::from(self.peak_relief.start_m) {
            return source_m * scale;
        }
        sea + self.peak_relief.apply(relative) * scale
    }

    pub fn source_height_above_sea_m(self, displayed_height_m: f64) -> f64 {
        self.peak_relief
            .inverse(displayed_height_m * 1000.0 / f64::from(self.height_scale_permille))
    }
}
