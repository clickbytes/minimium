//! Reduced SI atmospheric entry model. Presentation only; no impacts or CFD.
use glam::DVec3;

#[derive(Clone, Copy, Debug)]
pub enum MaterialKind {
    Stony,
    Iron,
    Brittle,
    Porous,
    Heterogeneous,
    Cracked,
}

#[derive(Clone, Copy, Debug)]
pub struct Material {
    pub kind: MaterialKind,
    pub density_kg_m3: f64,
    pub heat_j_kg: f64,
    pub heat_transfer: f64,
    pub erosion: f64,
    pub efficiency: f64,
    pub strength_pa: f64,
}

impl Material {
    pub fn seeded(seed: u64) -> Self {
        let (kind, density, heat, transfer, erosion, efficiency, strength) = match seed % 6 {
            0 => (MaterialKind::Stony, 3300.0, 6e6, 0.08, 0.03, 0.025, 3e6),
            1 => (MaterialKind::Iron, 7800.0, 8e6, 0.04, 0.0, 0.035, 20e6),
            2 => (MaterialKind::Brittle, 2200.0, 4e6, 0.12, 0.6, 0.02, 0.15e6),
            3 => (MaterialKind::Porous, 800.0, 3e6, 0.16, 1.2, 0.015, 0.03e6),
            4 => (
                MaterialKind::Heterogeneous,
                2900.0,
                5e6,
                0.1,
                0.2,
                0.03,
                1e6,
            ),
            _ => (MaterialKind::Cracked, 3000.0, 5e6, 0.09, 0.4, 0.025, 0.3e6),
        };
        Self {
            kind,
            density_kg_m3: density,
            heat_j_kg: heat,
            heat_transfer: transfer,
            erosion,
            efficiency,
            strength_pa: strength,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Body {
    pub position_m: DVec3,
    pub velocity_m_s: DVec3,
    pub mass_kg: f64,
    pub initial_mass_kg: f64,
    pub gas_power_w: f64,
    pub deposited_j: f64,
    pub pressure_pa: f64,
    pub ablated_kg: f64,
    pub eroded_kg: f64,
}

impl Body {
    pub fn radius_m(&self, material: Material) -> f64 {
        (3.0 * self.mass_kg.max(0.0) / (4.0 * std::f64::consts::PI * material.density_kg_m3)).cbrt()
    }

    /// Fixed solver substep, at most 10 ms. No unbounded catch-up loop here.
    /// Returns false for terminal/invalid states, retaining surviving dark mass.
    pub fn advance(&mut self, dt_s: f64, planet_radius_m: f64, material: Material) -> bool {
        if !dt_s.is_finite()
            || dt_s <= 0.0
            || dt_s > 0.01
            || !planet_radius_m.is_finite()
            || planet_radius_m <= 0.0
            || !self.position_m.is_finite()
            || !self.velocity_m_s.is_finite()
            || !self.mass_kg.is_finite()
            || self.mass_kg < 0.0
            || !self.initial_mass_kg.is_finite()
            || self.initial_mass_kg <= 0.0
            || !material.density_kg_m3.is_finite()
            || material.density_kg_m3 <= 0.0
            || !material.heat_j_kg.is_finite()
            || material.heat_j_kg <= 0.0
            || !(0.0..=1.0).contains(&material.heat_transfer)
            || !(0.0..=2.0).contains(&material.erosion)
            || !(0.0..=1.0).contains(&material.efficiency)
        {
            self.gas_power_w = 0.0;
            return false;
        }
        self.gas_power_w *= (-dt_s / 0.06).exp();
        self.deposited_j = 0.0;
        self.ablated_kg = 0.0;
        self.eroded_kg = 0.0;
        let height_m = self.position_m.length() - planet_radius_m;
        let speed = self.velocity_m_s.length();
        if height_m > 130_000.0
            && self.position_m.dot(self.velocity_m_s) > 0.0
            && self.gas_power_w < 0.01
        {
            self.gas_power_w = 0.0;
            return false;
        }
        if height_m < 35_000.0 || speed < 3000.0 || self.mass_kg < 1e-10 {
            return self.gas_power_w > 0.01;
        }
        let air_kg_m3 = 1.225 * (-height_m.max(0.0) / 7200.0).exp();
        let area_m2 = std::f64::consts::PI * self.radius_m(material).powi(2);
        self.pressure_pa = 0.5 * air_kg_m3 * speed * speed;
        let drag_n = self.pressure_pa * area_m2;
        let consumed = 1.0 - self.mass_kg / self.initial_mass_kg.max(1e-10);
        let layer = if matches!(material.kind, MaterialKind::Heterogeneous) {
            0.6 + 0.4 * (consumed * 40.0).sin()
        } else {
            1.0
        };
        let heat_w = drag_n * speed * material.heat_transfer * layer;
        let lost = (heat_w / material.heat_j_kg * (1.0 + material.erosion) * dt_s)
            .clamp(0.0, self.mass_kg);
        // Exponential drag cannot reverse the velocity in a stiff final step.
        let old_energy = 0.5 * self.mass_kg * speed * speed;
        let damping = (-drag_n * dt_s / (self.mass_kg * speed).max(1e-12)).exp();
        self.velocity_m_s *= damping;
        self.mass_kg = (self.mass_kg - lost).max(0.0);
        self.ablated_kg = lost / (1.0 + material.erosion);
        self.eroded_kg = lost - self.ablated_kg;
        let new_energy = 0.5 * self.mass_kg * self.velocity_m_s.length_squared();
        let carried_dust_energy = 0.5 * self.eroded_kg * self.velocity_m_s.length_squared();
        self.deposited_j = (old_energy - new_energy - carried_dust_energy).max(0.0);
        let power_w = self.deposited_j / dt_s.max(1e-9) * material.efficiency;
        self.gas_power_w += power_w * (1.0 - (-dt_s / 0.06).exp());
        self.position_m += self.velocity_m_s * dt_s;
        if !self.position_m.is_finite() || !self.gas_power_w.is_finite() {
            self.gas_power_w = 0.0;
            return false;
        }
        true
    }
}
