//! Seed-stable approximate visible emission; not calibrated absolute photometry.
use super::kinematics::{Body, Material};

#[derive(Clone, Copy, Debug, Default)]
pub struct Emission {
    pub rgb: [f64; 3],
    /// Isotropic visible power divided by 4 pi, in scene-linear units.
    pub intensity: f64,
    /// Effective luminous radius, distinct from the solid material radius.
    pub radius_m: f64,
    pub radiance: f64,
}

pub fn emission(body: Body, material: Material, age_s: f64, seed: u64) -> Emission {
    let phase = (seed >> 11) as f64 / (1u64 << 53) as f64 * std::f64::consts::TAU;
    // Continuous deterministic variation, independent of frame sampling.
    let modulation =
        1.0 + 0.06 * (age_s * 43.0 + phase).sin() + 0.025 * (age_s * 97.0 + phase * 1.7).sin();
    let power = (body.gas_power_w * modulation).clamp(0.0, 1e12);
    let radius = (body.radius_m(material) * 8.0 + power.sqrt() * 0.0005).clamp(0.005, 30.0);
    let intensity = power / (4.0 * std::f64::consts::PI);
    let palette = [
        [1.0, 0.94, 0.85],
        [1.0, 0.78, 0.45],
        [0.65, 1.0, 0.8],
        [0.65, 0.82, 1.0],
    ];
    let mut rgb = palette[(seed.rotate_right(17) % 4) as usize];
    let cooling = (body.velocity_m_s.length() / 35_000.0).clamp(0.0, 1.0);
    rgb[2] *= 0.8 + 0.2 * cooling;
    Emission {
        rgb,
        intensity,
        radius_m: radius,
        // Projected area supplies the inverse-square angular dilution for
        // unresolved cores; do not divide by camera distance a second time.
        radiance: (intensity / (std::f64::consts::PI * radius * radius)).min(1e12),
    }
}
