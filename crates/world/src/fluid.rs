//! Fluid observations consumed by movement and presentation, without a solver.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct WaterBodyId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WaterSample {
    pub submerged_fraction: f64,
    pub velocity_m_s: [f64; 3],
}

impl WaterSample {
    pub fn valid(self) -> bool {
        (0.0..=1.0).contains(&self.submerged_fraction)
            && self.velocity_m_s.iter().all(|v| v.is_finite())
    }
}

pub trait WaterRead: Send + Sync {
    fn sample_capsule(&self, feet_m: [f64; 3], up: [f64; 3], height_m: f64) -> WaterSample;
}
