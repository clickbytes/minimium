//! CPU cloud diagnostics and bounded tessellation policy, independent of GPU.
use glam::DVec3;
use minimium_environment::weather::Cloud;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum CloudDebug {
    #[default]
    Off,
    Regions,
    Layers,
    Wind,
    Kinds,
    Detail,
}

const MAX_LEVEL: u8 = 6;

/// One bounded sampling choice. The environment supplies physical bounds only.
#[derive(Clone, Copy)]
pub struct CloudTessellation {
    size_m: DVec3,
    minimum: u8,
    level: u8,
}

impl CloudTessellation {
    pub fn select(cloud: Cloud, requested: u8, sample_allowance: usize) -> Self {
        // The generated root is an exact power-of-two multiple of its support
        // quantum. Round, rather than ceil, avoids floating-point edge drift.
        let minimum = (cloud.size_m.max_element() / cloud.support_quantum_m)
            .log2()
            .round() as u8;
        let minimum = minimum.clamp(2, MAX_LEVEL);
        let mut result = Self {
            size_m: cloud.size_m,
            minimum,
            level: requested.clamp(minimum, MAX_LEVEL),
        };
        while result.level > minimum && result.sample_cost() > sample_allowance {
            result.level -= 1;
        }
        result
    }

    pub fn level(self) -> u8 {
        self.level
    }

    pub fn area_sampling(self) -> bool {
        self.level == self.minimum
    }

    pub fn grid(self) -> ([usize; 3], f64) {
        let cell = self.size_m.max_element() / f64::from(1_u32 << self.level);
        (
            self.size_m
                .to_array()
                .map(|v| (v / cell).round().max(1.0) as usize),
            cell,
        )
    }

    pub fn sample_cost(self) -> usize {
        self.grid().0.iter().product::<usize>() * if self.area_sampling() { 8 } else { 2 }
    }
}

/// Angular distance bands select target detail and relative mesh budget weight.
pub fn priority(cloud: Cloud, observer: DVec3, utc: f64) -> (u8, usize) {
    let distance = cloud
        .direction_at(utc)
        .dot(observer)
        .clamp(-1.0, 1.0)
        .acos();
    if distance < 0.006 {
        (6, 16)
    } else if distance < 0.018 {
        (5, 4)
    } else {
        (4, 1)
    }
}
