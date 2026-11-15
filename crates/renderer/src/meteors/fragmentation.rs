//! Bounded local breakup data. Slots are never reused within one event.
use super::kinematics::{Body, Material};
use glam::DVec3;

pub const SOLID_SLOTS: usize = 4;
pub const BODY_SLOTS: usize = 8;
pub const EVENT_CAP: usize = 64;

#[derive(Clone, Copy, Debug, Default)]
pub enum EventKind {
    #[default]
    Ablation,
    Erosion,
    Breakup,
    Flare,
    DustRelease,
    Extinction,
    DarkFlightEnd,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Event {
    pub kind: EventKind,
    pub at_s: f64,
    pub slot: u8,
    pub mass_kg: f64,
}

pub struct System {
    pub bodies: [Body; BODY_SLOTS],
    pub births: [f64; BODY_SLOTS],
    pub birth_bodies: [Body; BODY_SLOTS],
    generations: [u8; BODY_SLOTS],
    threshold: [f64; BODY_SLOTS],
    next_solid: usize,
    next_grain: usize,
    pub events: [Event; EVENT_CAP],
    pub event_count: usize,
    pub omitted_events: u64,
    pub ablated_kg: f64,
    pub unrepresented_dust_kg: f64,
    pub grain_packets: usize,
    seed: u64,
    previous_power: f64,
    last_flare_s: f64,
    terminal_recorded: bool,
}

impl System {
    pub fn new(body: Body, material: Material, seed: u64) -> Self {
        let mut bodies = [Body::default(); BODY_SLOTS];
        bodies[0] = body;
        let mut births = [f64::INFINITY; BODY_SLOTS];
        births[0] = 0.0;
        Self {
            bodies,
            births,
            birth_bodies: bodies,
            generations: [0; BODY_SLOTS],
            threshold: std::array::from_fn(|slot| {
                material.strength_pa * (0.65 + noise(seed, slot as u64))
            }),
            next_solid: 1,
            next_grain: SOLID_SLOTS,
            events: [Event::default(); EVENT_CAP],
            event_count: 0,
            omitted_events: 0,
            ablated_kg: 0.0,
            unrepresented_dust_kg: 0.0,
            grain_packets: 0,
            seed,
            previous_power: 0.0,
            last_flare_s: -1.0,
            terminal_recorded: false,
        }
    }

    fn record(&mut self, kind: EventKind, at_s: f64, slot: usize, mass_kg: f64) {
        let cap = match kind {
            EventKind::Extinction | EventKind::DarkFlightEnd => EVENT_CAP,
            EventKind::Ablation | EventKind::Erosion => EVENT_CAP - 10,
            _ => EVENT_CAP - 2,
        };
        if self.event_count >= cap {
            self.omitted_events = self.omitted_events.saturating_add(1);
        } else {
            self.events[self.event_count] = Event {
                kind,
                at_s,
                slot: slot as u8,
                mass_kg,
            };
            self.event_count += 1;
        }
    }

    pub fn advance(&mut self, at_s: f64, radius_m: f64, material: Material) -> bool {
        let mut alive = false;
        for slot in 0..BODY_SLOTS {
            if !self.births[slot].is_finite() || self.births[slot] >= at_s {
                continue;
            }
            let profile = material_for_slot(material, slot);
            let emitting = self.bodies[slot].advance(0.01, radius_m, profile);
            alive |= emitting;
            let body = self.bodies[slot];
            self.ablated_kg += body.ablated_kg;
            self.unrepresented_dust_kg += body.eroded_kg;
            if body.ablated_kg > 0.0 && (at_s / 0.25).floor() != ((at_s - 0.01) / 0.25).floor() {
                self.record(EventKind::Ablation, at_s, slot, body.ablated_kg);
            }
            if body.eroded_kg > 0.0
                && slot < SOLID_SLOTS
                && self.next_grain < BODY_SLOTS
                && self.unrepresented_dust_kg > body.initial_mass_kg * 0.005
            {
                let grain = self.next_grain;
                self.next_grain += 1;
                self.grain_packets += 1;
                let mass = self.unrepresented_dust_kg;
                self.unrepresented_dust_kg = 0.0;
                let packet = Body {
                    mass_kg: mass,
                    initial_mass_kg: mass,
                    gas_power_w: 0.0,
                    ..body
                };
                self.bodies[grain] = packet;
                self.birth_bodies[grain] = packet;
                self.births[grain] = at_s;
                self.record(EventKind::Erosion, at_s, slot, mass);
                self.record(EventKind::DustRelease, at_s, grain, mass);
            }
            if slot < SOLID_SLOTS
                && body.initial_mass_kg >= 0.02
                && body.mass_kg > 1e-6
                && body.pressure_pa >= self.threshold[slot]
                && self.generations[slot] < 2
                && self.next_solid < SOLID_SLOTS
            {
                self.breakup(slot, at_s);
            }
        }
        let power: f64 = self.bodies.iter().map(|body| body.gas_power_w).sum();
        if power > 100.0 && power > self.previous_power * 1.5 && at_s - self.last_flare_s > 0.12 {
            self.record(EventKind::Flare, at_s, 0, 0.0);
            self.last_flare_s = at_s;
        }
        self.previous_power = power;
        if !alive && !self.terminal_recorded {
            self.record(EventKind::Extinction, at_s, 0, 0.0);
            self.record(EventKind::DarkFlightEnd, at_s, 0, self.solid_mass_kg());
            self.terminal_recorded = true;
        }
        alive
    }

    fn breakup(&mut self, slot: usize, at_s: f64) {
        let child = self.next_solid;
        self.next_solid += 1;
        let parent = self.bodies[slot];
        let fraction = 0.2 + noise(self.seed, child as u64 + 20) * 0.25;
        let child_mass = parent.mass_kg * fraction;
        let axis = if parent.velocity_m_s.normalize_or_zero().y.abs() < 0.9 {
            DVec3::Y
        } else {
            DVec3::X
        };
        let first = parent.velocity_m_s.cross(axis).normalize_or_zero();
        let second = parent.velocity_m_s.normalize_or_zero().cross(first);
        let angle = std::f64::consts::TAU * noise(self.seed, child as u64 + 60);
        let spread = (first * angle.cos() + second * angle.sin())
            * (5.0 + 20.0 * noise(self.seed, child as u64 + 40));
        let dispersion_fraction = fraction * (1.0 - fraction) * spread.length_squared()
            / parent.velocity_m_s.length_squared().max(1.0);
        let forward = parent.velocity_m_s * (1.0 - dispersion_fraction).max(0.0).sqrt();
        self.bodies[slot].mass_kg = parent.mass_kg - child_mass;
        self.bodies[slot].velocity_m_s = forward - spread * fraction;
        self.bodies[slot].gas_power_w *= 1.0 - fraction;
        let daughter = Body {
            mass_kg: child_mass,
            initial_mass_kg: child_mass,
            velocity_m_s: forward + spread * (1.0 - fraction),
            gas_power_w: parent.gas_power_w * fraction,
            ..parent
        };
        self.bodies[child] = daughter;
        self.birth_bodies[child] = daughter;
        self.births[child] = at_s;
        self.generations[slot] += 1;
        self.generations[child] = self.generations[slot];
        self.threshold[slot] *= 3.0;
        self.threshold[child] *= 1.5;
        self.record(EventKind::Breakup, at_s, slot, child_mass);
    }

    pub fn solid_mass_kg(&self) -> f64 {
        self.bodies[..SOLID_SLOTS]
            .iter()
            .map(|body| body.mass_kg)
            .sum()
    }

    pub fn represented_dust_kg(&self) -> f64 {
        self.bodies[SOLID_SLOTS..]
            .iter()
            .map(|body| body.mass_kg)
            .sum()
    }
}

pub fn material_for_slot(mut material: Material, slot: usize) -> Material {
    if slot >= SOLID_SLOTS {
        // Effective area of a packet of fine grains, not a giant solid grain.
        material.density_kg_m3 = 100.0;
        material.erosion = 0.0;
    }
    material
}

fn noise(seed: u64, salt: u64) -> f64 {
    let mut value = seed ^ salt.wrapping_mul(0x9e3779b97f4a7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
    ((value ^ (value >> 31)) >> 11) as f64 / (1u64 << 53) as f64
}
