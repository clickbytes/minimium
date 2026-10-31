use super::coordinates::Cell;
use glam::{DQuat, DVec3};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub(crate) struct CloudId {
    pub(crate) seed: u128,
    pub(crate) cell: Cell,
    pub(crate) population: i64,
    pub(crate) candidate: u8,
    pub(crate) layer: u8,
    pub(crate) part_population: i64,
    pub(crate) path: u32,
}

impl CloudId {
    pub(crate) fn hash(self) -> u64 {
        let cell = self.cell;
        mix(self.seed as u64
            ^ (self.seed >> 64) as u64
            ^ (u64::from(cell.face) << 59)
            ^ (u64::from(cell.level) << 54)
            ^ (u64::from(cell.x) << 32)
            ^ (u64::from(cell.y) << 16)
            ^ mix(self.population as u64)
            ^ (u64::from(self.candidate) << 8)
            ^ u64::from(self.layer)
            ^ mix(self.part_population as u64)
            ^ mix(u64::from(self.path)))
    }
}

pub(crate) fn mix(mut key: u64) -> u64 {
    key = (key ^ (key >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    key = (key ^ (key >> 27)).wrapping_mul(0x94d049bb133111eb);
    key ^ (key >> 31)
}

pub(crate) fn random(key: u64, salt: u64) -> f64 {
    (mix(key ^ mix(salt)) >> 11) as f64 / ((1_u64 << 53) as f64)
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum CloudFamily {
    Cumulus,
    Stratocumulus,
    Stratus,
    Nimbostratus,
    Cirrus,
    Storm,
}

#[derive(Clone)]
pub(crate) struct CloudCluster {
    pub(crate) id: CloudId,
    pub(crate) family: CloudFamily,
    pub(crate) up: DVec3,
    pub(crate) base_m: f64,
    pub(crate) depth_m: f64,
    pub(crate) width_m: f64,
    pub(crate) convection: f64,
    pub(crate) fibrous: f64,
    pub(crate) mass: f64,
    pub(crate) wind: DVec3,
    pub(crate) shear: DVec3,
    pub(crate) rain_potential: f64,
    pub(crate) seconds: f64,
}

#[derive(Clone, Copy)]
pub(crate) struct CloudLobe {
    pub(crate) id: CloudId,
    pub(crate) center: DVec3,
    pub(crate) axes: DVec3,
    pub(crate) orientation: DQuat,
    /// Extinction per physical metre. Integral over volume is the allocated mass.
    pub(crate) density: f64,
    pub(crate) base_normal: DVec3,
    pub(crate) base_offset: f64,
    pub(crate) mass: f64,
    pub(crate) layer: u8,
    pub(crate) storm: f64,
    pub(crate) rain_potential: f64,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct CloudColumnSample {
    pub(crate) coverage: [f64; 3],
    pub(crate) optical_depth: [f64; 3],
    pub(crate) base_m: [f64; 3],
    pub(crate) top_m: [f64; 3],
    pub(crate) rain_mm_h: f64,
    pub(crate) storm: f64,
    pub(crate) storm_id: Option<CloudId>,
}
