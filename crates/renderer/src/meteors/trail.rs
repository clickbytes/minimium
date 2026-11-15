//! World-anchored deposition and bounded upper-atmosphere wind approximation.
use glam::DVec3;

pub const POINTS: usize = 64;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum TrailKind {
    #[default]
    Wake,
    Afterglow,
    Dust,
}

#[derive(Clone, Copy, Default)]
pub struct Deposit {
    pub at_s: f64,
    pub position_m: DVec3,
    pub direction: DVec3,
    pub visible_energy_j: f64,
    pub dust_kg: f64,
    pub serial: u32,
}

pub struct DepositRing {
    points: [Deposit; POINTS],
    next: usize,
    count: usize,
    pub overwritten: u64,
}

impl Default for DepositRing {
    fn default() -> Self {
        Self {
            points: [Deposit::default(); POINTS],
            next: 0,
            count: 0,
            overwritten: 0,
        }
    }
}

impl DepositRing {
    pub fn push(&mut self, point: Deposit) {
        if !point.at_s.is_finite()
            || !point.position_m.is_finite()
            || !point.direction.is_finite()
            || !point.visible_energy_j.is_finite()
            || !point.dust_kg.is_finite()
            || point.visible_energy_j < 0.0
            || point.dust_kg < 0.0
        {
            return;
        }
        if self.count == POINTS {
            self.overwritten = self.overwritten.saturating_add(1);
        }
        self.points[self.next] = point;
        self.next = (self.next + 1) % POINTS;
        self.count = (self.count + 1).min(POINTS);
    }

    pub fn iter(&self) -> impl Iterator<Item = &Deposit> {
        let first = (self.next + POINTS - self.count) % POINTS;
        (0..self.count).map(move |index| &self.points[(first + index) % POINTS])
    }
}

pub struct TrailProfile {
    pub glow_ttl_s: f64,
    pub dust_ttl_s: f64,
    pub seed: u64,
}

pub struct TrailSample {
    pub position_m: DVec3,
    pub width_m: f64,
    pub intensity: f64,
    pub opacity: f64,
}

impl TrailProfile {
    pub fn new(seed: u64, large: bool, allow_long: bool) -> Self {
        Self {
            glow_ttl_s: if large && allow_long && seed.is_multiple_of(4) {
                if seed.is_multiple_of(32) {
                    90.0
                } else {
                    20.0 + (seed % 41) as f64
                }
            } else {
                3.0
            },
            dust_ttl_s: if large && seed.is_multiple_of(3) {
                45.0
            } else {
                0.0
            },
            seed,
        }
    }

    pub fn ttl_s(&self) -> f64 {
        self.glow_ttl_s.max(self.dust_ttl_s)
    }

    pub fn sample(
        &self,
        deposit: &Deposit,
        now_s: f64,
        radius_m: f64,
        kind: TrailKind,
    ) -> Option<TrailSample> {
        let age = now_s - deposit.at_s;
        let ttl = if kind == TrailKind::Dust {
            self.dust_ttl_s
        } else {
            self.glow_ttl_s
        };
        if age < 0.0 || age >= ttl || ttl <= 0.0 {
            return None;
        }
        let height = (deposit.position_m.length() - radius_m).max(0.0);
        let up = deposit.position_m.normalize_or_zero();
        let axis = if up.z.abs() < 0.9 { DVec3::Z } else { DVec3::X };
        let east = axis.cross(up).normalize_or_zero();
        let north = up.cross(east);
        let shear = (height / 8000.0 + (self.seed % 1000) as f64 * 0.01).sin();
        // Tropospheric cloud winds do not describe mesospheric circulation.
        // This bounded 20..60 m/s profile is explicitly a local approximation.
        let wind = east * (40.0 + 20.0 * shear) + north * (10.0 * shear);
        let width = if kind == TrailKind::Dust {
            3.0 + age * 1.8
        } else {
            0.8 + age * 0.7
        };
        let end_fade = (1.0 - age / ttl).powi(2);
        Some(TrailSample {
            position_m: deposit.position_m + wind * age,
            width_m: width,
            intensity: if kind == TrailKind::Dust {
                0.0
            } else {
                deposit.visible_energy_j / (4.0 * std::f64::consts::PI * (ttl / 4.0))
                    * (-age / (ttl / 4.0)).exp()
                    * end_fade
            },
            opacity: if kind == TrailKind::Dust {
                (deposit.dust_kg * 150.0 / (std::f64::consts::PI * width * width)).min(0.12)
                    * end_fade
            } else {
                end_fade
            },
        })
    }
}
