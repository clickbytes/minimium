//! Human-readable preset adapter for the domain relief curve.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PeakReliefConfig {
    pub start_m: u32,
    pub full_m: u32,
    pub multiplier_permille: u32,
    pub exponent: u32,
}

impl PeakReliefConfig {
    pub fn definition(self) -> minimium_world::world::PeakRelief {
        minimium_world::world::PeakRelief {
            start_m: self.start_m,
            full_m: self.full_m,
            multiplier_permille: self.multiplier_permille,
            exponent: self.exponent,
        }
    }
}
