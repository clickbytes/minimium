//! Fixed, observer-independent physical history built once per cosmetic event.
use super::EventBasis;
use minimium_renderer::meteors::{
    kinematics::{Body, Material},
    MAX_METEOR_HISTORY_SAMPLES,
};

pub(super) const SAMPLE_DT_S: f64 = 0.04;

pub(super) struct Evolution {
    pub material: Material,
    samples: [Body; MAX_METEOR_HISTORY_SAMPLES],
    count: usize,
    peak_power_w: f64,
}

impl Evolution {
    pub fn new(
        basis: EventBasis,
        radius_m: f64,
        mass_kg: f64,
        duration_s: f64,
        seed: u64,
    ) -> Option<Self> {
        let speed = glam::DVec3::from_array(basis.velocity_m_s).length();
        if !radius_m.is_finite()
            || radius_m <= 0.0
            || !mass_kg.is_finite()
            || !(1e-7..=1e5).contains(&mass_kg)
            || !duration_s.is_finite()
            || !(0.0..=20.0).contains(&duration_s)
            || !(11_000.0..=72_000.0).contains(&speed)
        {
            return None;
        }
        let material = Material::seeded(seed);
        let mut body = Body {
            position_m: basis.start_phys_m.into(),
            velocity_m_s: basis.velocity_m_s.into(),
            mass_kg,
            initial_mass_kg: mass_kg,
            ..Body::default()
        };
        if !body.position_m.is_finite() || body.position_m.length() < radius_m + 35_000.0 {
            return None;
        }
        let mut result = Self {
            material,
            samples: [Body::default(); MAX_METEOR_HISTORY_SAMPLES],
            count: 1,
            peak_power_w: 0.0,
        };
        result.samples[0] = body;
        let steps = (duration_s / SAMPLE_DT_S).ceil() as usize;
        for _ in 0..steps.min(MAX_METEOR_HISTORY_SAMPLES - 1) {
            let mut alive = true;
            for _ in 0..4 {
                alive = body.advance(0.01, radius_m, material);
                if !alive {
                    break;
                }
            }
            result.samples[result.count] = body;
            result.peak_power_w = result.peak_power_w.max(body.gas_power_w);
            result.count += 1;
            if !alive {
                break;
            }
        }
        Some(result)
    }

    pub fn duration_s(&self) -> f64 {
        (self.count - 1) as f64 * SAMPLE_DT_S
    }

    pub fn phase(&self, age_s: f64) -> minimium_renderer::meteors::model::MeteorPhase {
        use minimium_renderer::meteors::model::MeteorPhase;
        let power = self.sample(age_s).gas_power_w;
        if age_s > 0.0 && power < 0.01 {
            MeteorPhase::Fading
        } else if power > self.peak_power_w * 0.8 && power > 0.0 {
            MeteorPhase::Peak
        } else if power >= self.sample((age_s - SAMPLE_DT_S).max(0.0)).gas_power_w {
            MeteorPhase::Incoming
        } else {
            MeteorPhase::Fading
        }
    }

    pub fn sample(&self, age_s: f64) -> Body {
        let phase = (age_s / SAMPLE_DT_S).clamp(0.0, (self.count - 1) as f64);
        let index = phase.floor() as usize;
        let a = self.samples[index];
        let b = self.samples[(index + 1).min(self.count - 1)];
        let t = phase.fract();
        Body {
            position_m: a.position_m.lerp(b.position_m, t),
            velocity_m_s: a.velocity_m_s.lerp(b.velocity_m_s, t),
            mass_kg: a.mass_kg + (b.mass_kg - a.mass_kg) * t,
            gas_power_w: a.gas_power_w + (b.gas_power_w - a.gas_power_w) * t,
            ..a
        }
    }
}
