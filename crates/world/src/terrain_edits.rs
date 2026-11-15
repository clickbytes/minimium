//! Versioned, immutable local changes to a planet's base surface.
//! Positions and dimensions are game-world metres, independent of render origins.

pub const TERRAIN_EDIT_VERSION: u16 = 1;
pub const MAX_TERRAIN_EDITS: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CraterStamp {
    pub id: u64,
    pub center: [f64; 3],
    pub radius_m: f64,
    pub depth_m: f64,
    pub seed: u64,
}

impl CraterStamp {
    pub fn valid(&self) -> bool {
        self.id != 0
            && self
                .center
                .iter()
                .all(|v| v.is_finite() && v.abs() < 10_000_000.0)
            && length(self.center) > 1.0
            && self.radius_m.is_finite()
            && (6.0..=80.0).contains(&self.radius_m)
            && self.depth_m.is_finite()
            && (0.2..=self.radius_m * 0.4).contains(&self.depth_m)
    }

    /// Smooth bowl, raised rim and ejecta, with continuous seeded asymmetry.
    pub fn displacement(&self, direction: [f64; 3], planet_radius: f64) -> (f64, f64) {
        let center = project(self.center, planet_radius);
        let point = project(direction, planet_radius);
        let delta = std::array::from_fn::<_, 3, _>(|i| point[i] - center[i]);
        let r = length(delta) / self.radius_m;
        if r >= 2.0 {
            return (0.0, 0.0);
        }
        let phase = (self.seed % 65521) as f64 * 0.017;
        let irregularity = 1.0
            + 0.06 * (delta[0] * 0.19 + phase).sin() * (delta[1] * 0.23 - delta[2] * 0.17).cos();
        let bowl = if r < 1.0 { -(1.0 - r * r).powi(2) } else { 0.0 };
        let rim = 0.22 * (-(r - 1.0).powi(2) / 0.035).exp();
        let edge = ((2.0 - r) / 0.35).clamp(0.0, 1.0);
        let fade = edge * edge * (3.0 - 2.0 * edge);
        (
            (bowl + rim) * self.depth_m * irregularity * fade,
            ((2.0 - r) / 0.9).clamp(0.0, 1.0),
        )
    }
}

pub fn length(p: [f64; 3]) -> f64 {
    p.iter().map(|v| v * v).sum::<f64>().sqrt()
}
pub fn project(p: [f64; 3], radius: f64) -> [f64; 3] {
    let scale = radius / length(p).max(1e-20);
    p.map(|v| v * scale)
}
