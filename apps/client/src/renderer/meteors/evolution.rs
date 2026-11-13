//! Pooled fixed histories; initialization performs no allocation per event.
use super::EventBasis;
use minimium_renderer::meteors::{
    fragmentation::{Event, System, BODY_SLOTS, EVENT_CAP},
    kinematics::{Body, Material},
    MAX_METEOR_HISTORY_SAMPLES,
};

pub(super) const SAMPLE_DT_S: f64 = 0.04;

pub(super) struct Evolution {
    pub material: Material,
    samples: Box<[[Body; BODY_SLOTS]]>,
    count: usize,
    peak_power_w: [f64; BODY_SLOTS],
    pub births: [f64; BODY_SLOTS],
    birth_bodies: [Body; BODY_SLOTS],
    pub events: [Event; EVENT_CAP],
    pub event_count: usize,
    pub omitted_events: u64,
    pub grain_packets: usize,
    pub mass_ledger: [f64; 4],
}

impl Evolution {
    /// Called exactly eight times at runtime creation, not at spawn.
    pub fn empty() -> Self {
        Self {
            material: Material::seeded(0),
            samples: vec![[Body::default(); BODY_SLOTS]; MAX_METEOR_HISTORY_SAMPLES]
                .into_boxed_slice(),
            count: 1,
            peak_power_w: [0.0; BODY_SLOTS],
            births: [f64::INFINITY; BODY_SLOTS],
            birth_bodies: [Body::default(); BODY_SLOTS],
            events: [Event::default(); EVENT_CAP],
            event_count: 0,
            omitted_events: 0,
            grain_packets: 0,
            mass_ledger: [0.0; 4],
        }
    }

    pub fn initialize(
        &mut self,
        basis: EventBasis,
        radius_m: f64,
        mass_kg: f64,
        seed: u64,
        preset: crate::client_commands::MeteorPreset,
    ) -> bool {
        let speed = glam::DVec3::from_array(basis.velocity_m_s).length();
        if !radius_m.is_finite()
            || radius_m <= 0.0
            || !mass_kg.is_finite()
            || !(1e-7..=1e5).contains(&mass_kg)
            || !(11_000.0..=72_000.0).contains(&speed)
        {
            return false;
        }
        let material = super::presets::material(preset, seed);
        let body = Body {
            position_m: basis.start_phys_m.into(),
            velocity_m_s: basis.velocity_m_s.into(),
            mass_kg,
            initial_mass_kg: mass_kg,
            ..Body::default()
        };
        if !body.position_m.is_finite() || body.position_m.length() < radius_m + 35_000.0 {
            return false;
        }
        let mut system = System::new(body, material, seed);
        self.material = material;
        self.count = 1;
        self.peak_power_w = [0.0; BODY_SLOTS];
        self.samples[0] = system.bodies;
        for sample in 1..=500 {
            let mut alive = true;
            for substep in 1..=4 {
                let at_s = ((sample - 1) * 4 + substep) as f64 * 0.01;
                alive = system.advance(at_s, radius_m, material);
                if !alive {
                    break;
                }
            }
            self.samples[self.count] = system.bodies;
            for slot in 0..BODY_SLOTS {
                self.peak_power_w[slot] =
                    self.peak_power_w[slot].max(system.bodies[slot].gas_power_w);
            }
            self.count += 1;
            if !alive {
                break;
            }
        }
        self.births = system.births;
        self.birth_bodies = system.birth_bodies;
        self.events = system.events;
        self.event_count = system.event_count;
        self.omitted_events = system.omitted_events;
        self.grain_packets = system.grain_packets;
        self.mass_ledger = [
            system.solid_mass_kg(),
            system.represented_dust_kg(),
            system.unrepresented_dust_kg,
            system.ablated_kg,
        ];
        true
    }

    pub fn duration_s(&self) -> f64 {
        (self.count - 1) as f64 * SAMPLE_DT_S
    }

    pub fn phase(&self, age_s: f64, slot: usize) -> minimium_renderer::meteors::model::MeteorPhase {
        use minimium_renderer::meteors::model::MeteorPhase;
        let power = self.sample_body(age_s, slot).gas_power_w;
        if age_s > 0.0 && power < 0.01 {
            MeteorPhase::Fading
        } else if power > self.peak_power_w[slot] * 0.8 && power > 0.0 {
            MeteorPhase::Peak
        } else if power
            >= self
                .sample_body((age_s - SAMPLE_DT_S).max(0.0), slot)
                .gas_power_w
        {
            MeteorPhase::Incoming
        } else {
            MeteorPhase::Fading
        }
    }

    pub fn sample(&self, age_s: f64) -> Body {
        self.sample_body(age_s, 0)
    }

    pub fn sample_body(&self, age_s: f64, slot: usize) -> Body {
        if age_s < self.births[slot] {
            return Body::default();
        }
        let phase = (age_s / SAMPLE_DT_S).clamp(0.0, (self.count - 1) as f64);
        let index = phase.floor() as usize;
        let mut a = self.samples[index][slot];
        let b = self.samples[(index + 1).min(self.count - 1)][slot];
        let mut t = phase.fract();
        if self.births[slot] > index as f64 * SAMPLE_DT_S {
            a = self.birth_bodies[slot];
            let end_s = (index + 1) as f64 * SAMPLE_DT_S;
            t = ((age_s - self.births[slot]) / (end_s - self.births[slot]).max(1e-9))
                .clamp(0.0, 1.0);
        }
        Body {
            position_m: a.position_m.lerp(b.position_m, t),
            velocity_m_s: a.velocity_m_s.lerp(b.velocity_m_s, t),
            mass_kg: a.mass_kg + (b.mass_kg - a.mass_kg) * t,
            gas_power_w: a.gas_power_w + (b.gas_power_w - a.gas_power_w) * t,
            ..a
        }
    }
}
