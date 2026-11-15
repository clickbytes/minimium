//! Initial conditions only; every family uses the same physical solver.
use super::{LightParams, SpawnParams};
use crate::client_commands::MeteorPreset;
use minimium_renderer::meteors::kinematics::Material;

impl SpawnParams {
    /// Constant preset table; `faint`/`bright`/`globe` labels match
    /// `MeteorPreset::parse` in the command layer.
    pub(super) const fn of(preset: MeteorPreset) -> Self {
        match preset {
            MeteorPreset::Faint => Self {
                speed_m_s: 18.0e3,
                duration_s: 2.2,
                mass_kg: 0.002,
                trail_thickness_m: 0.8,
                trail_opacity: 0.72,
                light: None,
            },
            MeteorPreset::Bright => Self {
                speed_m_s: 32.0e3,
                duration_s: 3.6,
                mass_kg: 0.5,
                trail_thickness_m: 1.6,
                trail_opacity: 0.9,
                light: Some(LightParams { range_m: 6.0e4 }),
            },
            MeteorPreset::Fragmenting => Self {
                duration_s: 1.0,
                ..Self::of(MeteorPreset::Fireball)
            },
            MeteorPreset::Fireball => Self {
                speed_m_s: 55.0e3,
                duration_s: 5.2,
                mass_kg: 100.0,
                trail_thickness_m: 3.2,
                trail_opacity: 1.0,
                light: Some(LightParams { range_m: 1.5e5 }),
            },
            other => {
                let (mass, speed, _, _, _) = conditions(other);
                Self {
                    mass_kg: mass,
                    speed_m_s: speed,
                    ..Self::of(MeteorPreset::Fireball)
                }
            }
        }
    }
}

// mass kg, entry m/s, material family, start altitude m, radial velocity fraction.
const fn conditions(p: MeteorPreset) -> (f64, f64, u64, f64, f64) {
    use MeteorPreset::*;
    match p {
        Ablation => (0.2, 22000., 0, 100000., 0.2),
        Burnout => (0.005, 65000., 3, 105000., 0.7),
        Erosion => (2., 28000., 3, 95000., 0.4),
        Droplets => (5., 26000., 5, 85000., 0.3),
        Irregular => (8., 32000., 4, 100000., 0.35),
        SingleSplit => (30., 26000., 2, 90000., 0.5),
        AsymmetricSplit => (50., 30000., 5, 85000., 0.55),
        MultipleSplit => (80., 40000., 2, 95000., 0.65),
        Cascade => (150., 50000., 3, 100000., 0.7),
        Dustball => (20., 24000., 3, 90000., 0.25),
        HardCore => (100., 18000., 1, 85000., 0.65),
        SingleFlare => (60., 38000., 0, 90000., 0.6),
        MultipleFlares => (180., 45000., 4, 100000., 0.55),
        TerminalFlare => (200., 28000., 5, 80000., 0.8),
        Airburst => (5000., 32000., 2, 85000., 0.8),
        Remnants => (300., 18000., 5, 80000., 0.5),
        DarkFlight => (0.5, 12000., 1, 75000., 0.8),
        Debris => (80., 35000., 3, 85000., 0.45),
        LowFlare => (10000., 20000., 0, 65000., 0.8),
        Grazing => (100., 60000., 1, 90000., 0.005),
        Fragmenting => (100., 55000., 2, 82000., 0.7),
        _ => (0., 0., 0, 0., 0.),
    }
}

pub(super) fn material(p: MeteorPreset, seed: u64) -> Material {
    match p {
        MeteorPreset::Faint | MeteorPreset::Bright | MeteorPreset::Fireball => {
            Material::seeded(seed)
        }
        _ => Material::seeded(conditions(p).2),
    }
}
pub(super) fn entry(p: MeteorPreset) -> Option<(f64, f64)> {
    let (_, _, _, height, sink) = conditions(p);
    (height > 0.).then_some((height, sink))
}
