//! Reduced atmospheric-entry dynamics. SI internally; no world/impact effects.
use glam::DVec3;

pub const BODY_COUNT: usize = 20;
pub const SOLID_COUNT: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BolidFamily {
    Smooth,
    Eroding,
    Burst,
    Cascade,
    Intermittent,
    TerminalFlare,
}

pub fn variation(seed: f64, salt: u32) -> f64 {
    let mut value = (seed * 65535.0).round() as u32 ^ salt;
    value = (value ^ (value >> 16)).wrapping_mul(0x7feb352d);
    value = (value ^ (value >> 15)).wrapping_mul(0x846ca68b);
    f64::from(value ^ (value >> 16)) / f64::from(u32::MAX)
}

#[derive(Clone, Copy)]
pub struct Material {
    pub family: BolidFamily,
    pub density: f64,
    pub strength: f64,
    pub ablation: f64,
    pub erosion: f64,
    pub seed: f64,
}

impl Material {
    pub fn new(seed: f64, kind: usize) -> Self {
        let families = [
            BolidFamily::Smooth,
            BolidFamily::Eroding,
            BolidFamily::Burst,
            BolidFamily::Cascade,
            BolidFamily::Intermittent,
            BolidFamily::TerminalFlare,
        ];
        let family = families[(variation(seed, 19) * 6.0) as usize % 6];
        let strength = match family {
            BolidFamily::Smooth => 8e6,
            BolidFamily::Eroding => 1e5,
            BolidFamily::Burst | BolidFamily::TerminalFlare => 3e5,
            BolidFamily::Cascade => 2e4,
            BolidFamily::Intermittent => 7e4,
        } * (0.6 + variation(seed, 3))
            * (0.8 + kind as f64 * 0.2);
        Self {
            family,
            seed,
            strength,
            density: 900.0 + variation(seed, 2) * 2600.0,
            // Intrinsic ablation, with fragmentation/erosion handled separately.
            // The old apparent 0.01..0.03 coefficient removed most material
            // before pressure failures. SI: 0.003..0.007 kg/MJ -> kg/J.
            ablation: 3e-9 + variation(seed, 4) * 4e-9,
            erosion: match family {
                BolidFamily::Eroding => 1.8,
                BolidFamily::Cascade => 0.6,
                BolidFamily::Intermittent => 0.35,
                _ => 0.08,
            },
        }
    }
}

#[derive(Clone, Copy, Default)]
pub struct Body {
    pub position: DVec3,
    pub birth_position: DVec3,
    pub birth_power: f64,
    pub velocity: DVec3,
    pub mass: f64,
    pub initial_mass: f64,
    pub power: f64,
    pub gas: f64,
    pub temperature: f64,
    pub strength: f64,
    pub damage: f64,
    pub age: f64,
    pub generation: u32,
    pub serial: u32,
    pub multiplicity: f64,
    pub birth: f64,
    pub last_break: f64,
    pub last_shed: f64,
    pub shed: f64,
    pub deposited: f64,
}

impl Body {
    pub fn advance(&mut self, dt: f64, radius: f64, material: Material) {
        self.deposited = 0.0;
        if self.serial == 0 {
            return;
        }
        self.age += dt;
        let old_gas = self.gas;
        self.gas *= (-dt / 0.055).exp();
        self.power = self.gas;
        if self.mass <= 1e-10 {
            self.temperature *= (-dt / 0.12).exp();
            return;
        }
        let altitude = (self.position.length() - radius).max(0.0);
        let air = 1.225 * (-altitude / 7200.0).exp();
        let speed = self.velocity.length();
        let pressure = air * speed * speed;
        let thermal_target = (air * speed.powi(3) / 1e8).clamp(0.0, 1.0);
        self.temperature += (thermal_target - self.temperature) * (1.0 - (-dt / 0.035).exp());
        let area = std::f64::consts::PI
            * (3.0 * self.mass / (4.0 * std::f64::consts::PI * material.density)).powf(2.0 / 3.0)
            * self.multiplicity.max(1.0).cbrt();
        let consumed = (1.0 - self.mass / self.initial_mass.max(1e-10)).clamp(0.0, 1.0);
        // Alternating refractory and volatile layers depend on consumed mass,
        // not a frame-time oscillator. Pressure failures expose the next layer.
        let layer = if material.family == BolidFamily::Intermittent {
            let phase = (consumed * 9.0 + self.generation as f64 * 0.31).fract();
            let edge =
                ((phase - 0.25) / 0.12).clamp(0.0, 1.0) * ((0.9 - phase) / 0.12).clamp(0.0, 1.0);
            0.025 + 0.975 * edge
        } else {
            1.0
        };
        let activation = self.temperature * ((speed - 3000.0) / 7000.0).clamp(0.0, 1.0);
        let energy = 0.5 * self.mass * speed * speed;
        let vapor = (material.ablation * air * area * speed.powi(3) * dt * layer * activation)
            .min(self.mass * 0.25);
        let grains = if self.multiplicity <= 1.0 {
            (vapor * material.erosion * (pressure / material.strength).min(2.0))
                .min(self.mass * 0.1)
        } else {
            0.0
        };
        let braking = (0.5 * air * area * speed * speed / self.mass * dt).min(speed * 0.15);
        self.mass -= vapor + grains;
        self.shed += grains;
        self.deposited = vapor;
        self.velocity *= (speed - braking) / speed.max(1.0);
        // Shed grains retain kinetic energy; do not count it twice as radiation.
        let remaining_energy = 0.5 * (self.mass + grains) * self.velocity.length_squared();
        let efficiency = 0.003 + 0.032 * activation;
        // Smooth seeded fluctuations are shared by the sky and point source.
        // Their small amplitude preserves the mass-driven flare/gap envelope.
        let phase = material.seed * std::f64::consts::TAU;
        let time = self.age + self.birth;
        let modulation =
            1.0 + 0.06 * (time * 43.0 + phase).sin() + 0.025 * (time * 97.0 + phase * 1.7).sin();
        let emission =
            ((energy - remaining_energy) / dt).max(0.0) * efficiency * activation * modulation;
        self.gas = old_gas + (emission - old_gas) * (1.0 - (-dt / 0.018).exp());
        self.power = self.gas;
        self.damage += (pressure / self.strength.max(1.0) - 1.0).max(0.0) * dt;
        self.velocity -= self.position.normalize_or_zero() * (9.81 * dt);
        self.position += self.velocity * dt;
        if self.position.length() <= radius {
            self.deposited += self.mass;
            self.mass = 0.0;
        }
    }
}

pub struct Dynamics {
    pub bodies: [Body; BODY_COUNT],
    pub material: Material,
    pub vaporized: f64,
    pub unresolved_grains: f64,
    pub fractures: u32,
    pub grain_releases: u32,
    serial: u32,
}

impl Dynamics {
    pub fn new(start: DVec3, velocity: DVec3, mass: f64, material: Material) -> Self {
        let mut bodies = [Body::default(); BODY_COUNT];
        bodies[0] = Body {
            position: start,
            birth_position: start,
            velocity,
            mass,
            initial_mass: mass,
            strength: material.strength,
            multiplicity: 1.0,
            serial: 1,
            ..Default::default()
        };
        Self {
            bodies,
            material,
            vaporized: 0.0,
            unresolved_grains: 0.0,
            fractures: 0,
            grain_releases: 0,
            serial: 1,
        }
    }

    pub fn step(&mut self, time: f64, dt: f64, radius: f64) {
        for body in &mut self.bodies {
            body.advance(dt, radius, self.material);
            self.vaporized += body.deposited;
        }
        for index in 0..SOLID_COUNT {
            let body = self.bodies[index];
            let generations = match self.material.family {
                BolidFamily::Smooth => 0,
                BolidFamily::Burst | BolidFamily::Eroding => 1,
                _ => 3,
            };
            if body.mass > 1e-7
                && body.damage > 0.025
                && body.generation < generations
                && time - body.last_break > 0.07
            {
                self.fracture(index, time);
            }
            if self.bodies[index].shed > 1e-9 && (time - body.last_shed > 0.04 || body.mass < 1e-7)
            {
                self.release_grains(index, time);
            }
        }
    }

    fn vacant(&self, range: std::ops::Range<usize>) -> Option<usize> {
        range
            .into_iter()
            .find(|&i| self.bodies[i].mass < 1e-10 && self.bodies[i].gas < 10.0)
    }

    fn fracture(&mut self, index: usize, time: f64) {
        let Some(slot) = self.vacant(1..SOLID_COUNT) else {
            return;
        };
        let parent = self.bodies[index];
        let ratio = 0.25 + variation(self.material.seed, self.serial + 30) * 0.3;
        let axis = parent.velocity.normalize_or_zero();
        let lateral = axis
            .cross(parent.position.normalize_or_zero())
            .normalize_or_zero();
        // Breakup directions occupy the transverse plane, not a single fixed
        // axis that collapses all daughter wakes when viewed edge-on.
        let transverse = axis.cross(lateral).normalize_or_zero();
        let azimuth = variation(self.material.seed, self.serial + 91) * std::f64::consts::TAU;
        let spread = (lateral * azimuth.cos() + transverse * azimuth.sin())
            * (30.0 + variation(self.material.seed, self.serial + 90) * 120.0);
        let child_mass = parent.mass * ratio;
        let cascade = matches!(
            self.material.family,
            BolidFamily::Cascade | BolidFamily::TerminalFlare
        );
        let strength_gain = if cascade { 1.25 } else { 2.8 };
        self.serial += 1;
        self.bodies[slot] = Body {
            mass: child_mass,
            birth_position: parent.position,
            initial_mass: child_mass,
            velocity: parent.velocity + spread,
            generation: parent.generation + 1,
            strength: parent.strength * strength_gain,
            damage: 0.0,
            age: 0.0,
            birth: time,
            last_break: time,
            shed: 0.0,
            deposited: 0.0,
            power: parent.power * ratio,
            birth_power: parent.power * ratio,
            gas: parent.gas * ratio,
            serial: self.serial,
            ..parent
        };
        let remaining = &mut self.bodies[index];
        remaining.mass -= child_mass;
        remaining.initial_mass = remaining.mass;
        remaining.velocity -= spread * ratio / (1.0 - ratio);
        remaining.gas *= 1.0 - ratio;
        remaining.power *= 1.0 - ratio;
        remaining.generation += 1;
        remaining.strength *= strength_gain;
        remaining.damage = 0.0;
        remaining.last_break = time;
        self.fractures += 1;
    }

    fn release_grains(&mut self, index: usize, time: f64) {
        let parent = self.bodies[index];
        let Some(slot) = self.vacant(SOLID_COUNT..BODY_COUNT) else {
            // Unresolved erosion still becomes deposited matter, never energy
            // or additional solid mass. The finite pool cannot grow unbounded.
            self.unresolved_grains += parent.shed;
            self.bodies[index].deposited += parent.shed;
            self.bodies[index].shed = 0.0;
            self.bodies[index].last_shed = time;
            return;
        };
        self.serial += 1;
        self.bodies[slot] = Body {
            mass: parent.shed,
            birth_position: parent.position,
            initial_mass: parent.shed,
            multiplicity: 30.0 + variation(self.material.seed, self.serial) * 170.0,
            strength: f64::MAX,
            generation: 4,
            age: 0.0,
            birth: time,
            gas: 0.0,
            power: 0.0,
            birth_power: 0.0,
            damage: 0.0,
            shed: 0.0,
            deposited: 0.0,
            serial: self.serial,
            ..parent
        };
        self.bodies[index].shed = 0.0;
        self.bodies[index].last_shed = time;
        self.grain_releases += 1;
    }
}
