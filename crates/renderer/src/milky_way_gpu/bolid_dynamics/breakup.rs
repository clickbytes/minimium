//! Bounded pressure-driven fragmentation with a broad daughter mass spectrum.
use super::{variation, Body, BolidFamily, Dynamics, SOLID_COUNT};
use glam::DVec3;

impl Dynamics {
    pub(super) fn fracture(&mut self, index: usize, time: f64) {
        let parent = self.bodies[index];
        let (daughters, released) = match self.material.family {
            BolidFamily::Burst => (3, 0.65),
            BolidFamily::TerminalFlare => (5, 0.82),
            BolidFamily::Cascade if parent.generation == 0 => (3, 0.6),
            BolidFamily::Cascade => (2, 0.55),
            BolidFamily::Eroding => (1, 0.25),
            _ => (1, 0.4),
        };
        let slots: Vec<_> = (1..SOLID_COUNT)
            .filter(|&i| self.bodies[i].mass < 1e-10 && self.bodies[i].gas < 10.0)
            .take(daughters)
            .collect();
        if slots.is_empty() {
            return;
        }
        // Equal-sized binary fragments have similar ballistic coefficients and
        // remain unresolved. Small daughters decelerate into their own wakes.
        let weights: [f64; 5] = std::array::from_fn(|i| {
            [1.0, 0.22, 0.025, 0.006, 0.0015][i]
                * (0.75 + variation(self.material.seed, self.serial + i as u32 + 30) * 0.5)
        });
        let total: f64 = weights[..slots.len()].iter().sum();
        let fractions = weights.map(|weight| weight / total * released);
        let axis = parent.velocity.normalize_or_zero();
        // A radial entry still needs a full transverse plane. Crossing with
        // the radial direction previously collapsed to zero for that case.
        let reference = if axis.y.abs() < 0.9 {
            DVec3::Y
        } else {
            DVec3::X
        };
        let lateral = axis.cross(reference).normalize_or_zero();
        let transverse = axis.cross(lateral);
        let phase = variation(self.material.seed, self.serial + 91) * std::f64::consts::TAU;
        let mut offsets: [DVec3; 5] = std::array::from_fn(|i| {
            let angle = phase + i as f64 * 2.399963229728653;
            (lateral * angle.cos() + transverse * angle.sin())
                * (30.0 + variation(self.material.seed, self.serial + i as u32 + 90) * 120.0)
        });
        let mut recoil = -slots
            .iter()
            .enumerate()
            .fold(DVec3::ZERO, |sum, (i, _)| sum + offsets[i] * fractions[i])
            / (1.0 - released);
        let dispersion_energy = recoil.length_squared() * (1.0 - released)
            + slots
                .iter()
                .enumerate()
                .map(|(i, _)| offsets[i].length_squared() * fractions[i])
                .sum::<f64>();
        // Aerodynamic dispersion spends forward kinetic energy. Transverse
        // momentum cancels; the split does not manufacture a luminous impulse.
        let speed_squared = parent.velocity.length_squared();
        let dispersion_fraction = (dispersion_energy / speed_squared.max(1e-12)).min(0.01);
        let scale = (dispersion_fraction * speed_squared / dispersion_energy.max(1e-12)).sqrt();
        for offset in &mut offsets {
            *offset *= scale;
        }
        recoil *= scale;
        let forward = parent.velocity * (1.0 - dispersion_fraction).sqrt();
        let cascading = matches!(
            self.material.family,
            BolidFamily::Cascade | BolidFamily::TerminalFlare
        );
        let strength_gain = if cascading { 1.25 } else { 2.8 };
        for (i, slot) in slots.into_iter().enumerate() {
            self.serial += 1;
            let mass = parent.mass * fractions[i];
            self.bodies[slot] = Body {
                mass,
                initial_mass: mass,
                birth_position: parent.position,
                velocity: forward + offsets[i],
                generation: parent.generation + 1,
                strength: parent.strength
                    * strength_gain
                    * (0.85 + variation(self.material.seed, self.serial + 120) * 0.3),
                damage: 0.0,
                age: 0.0,
                birth: time,
                last_break: time,
                last_shed: time,
                shed: 0.0,
                deposited: 0.0,
                power: parent.power * fractions[i],
                birth_power: parent.power * fractions[i],
                gas: parent.gas * fractions[i],
                serial: self.serial,
                ..parent
            };
        }
        let remaining = &mut self.bodies[index];
        remaining.mass = parent.mass * (1.0 - released);
        remaining.initial_mass = remaining.mass;
        remaining.velocity = forward + recoil;
        remaining.gas *= 1.0 - released;
        remaining.power *= 1.0 - released;
        remaining.generation += 1;
        remaining.strength *= strength_gain;
        remaining.damage = 0.0;
        remaining.last_break = time;
        self.fractures += 1;
    }
}
