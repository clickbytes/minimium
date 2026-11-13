//! Cached mass-driven bolid presentation; no authoritative collision effects.
use super::{
    bolid_dynamics::{self, variation, BolidFamily, Material},
    bolid_history::{self, Emitter, EvolutionDiagnostics, Snapshot, FRAME_DT},
    BolidGpuEvent,
};
use glam::{DVec3, Vec3};

// Keep the fresh-wake handoff in sync with bolid.wgsl.
const FRESH_WAKE_SECONDS: f32 = 0.25;
const FRESH_WAKE_SHARE: f32 = 0.7;

pub struct BolidEvolution {
    frames: Vec<Snapshot>,
    material: Material,
    diagnostics: EvolutionDiagnostics,
    side: Vec3,
    up: Vec3,
    knots: Vec<usize>,
    length_scale: f32,
    physical_radius: f32,
}

impl BolidEvolution {
    pub fn new(event: &BolidGpuEvent, radius: f64, length_scale: f64) -> Self {
        let length_scale = length_scale.clamp(0.001, 1.0);
        let start =
            Vec3::from_array(event.start_age[..3].try_into().unwrap()).as_dvec3() / length_scale;
        let end =
            Vec3::from_array(event.end_duration[..3].try_into().unwrap()).as_dvec3() / length_scale;
        let duration = f64::from(event.end_duration[3]).max(0.01);
        let seed = f64::from(event.style[1]);
        let kind = (event.style[0] as usize).min(2);
        let material = Material::new(seed, kind);
        let mass = [0.002, 0.8, 18.0][kind] * (0.5 + variation(seed, 1));
        let axis = (end - start).normalize_or_zero();
        let side = axis.cross(start.normalize_or_zero()).normalize_or_zero();
        let (frames, diagnostics) = bolid_history::build(
            start,
            (end - start) / duration,
            mass,
            material,
            radius / length_scale,
        );
        let knots = bolid_history::trail_knots(&frames, 24);
        Self {
            frames,
            material,
            diagnostics,
            side: side.as_vec3(),
            up: axis.cross(side).as_vec3(),
            knots,
            length_scale: length_scale as f32,
            physical_radius: (radius / length_scale) as f32,
        }
    }

    pub fn family(&self) -> BolidFamily {
        self.material.family
    }
    pub fn diagnostics(&self) -> EvolutionDiagnostics {
        self.diagnostics
    }
    pub fn luminous_end(&self) -> f32 {
        self.diagnostics.luminous_end_s
    }

    pub fn sample_emitters(&self, age: f32) -> [Emitter; bolid_dynamics::BODY_COUNT] {
        let position = f64::from(age.max(0.0)) / FRAME_DT;
        let index = (position.floor() as usize).min(self.frames.len() - 1);
        let a = &self.frames[index];
        let b = &self.frames[(index + 1).min(self.frames.len() - 1)];
        let t = position.fract() as f32;
        let end_age = ((index + 1).min(self.frames.len() - 1) as f64 * FRAME_DT) as f32;
        let mut emitters = std::array::from_fn(|i| {
            Emitter::interpolate(a.emitters[i], b.emitters[i], t, age, end_age)
        });
        // Birth-aware interpolation redistributes emission between bodies.
        // Preserve the cached total light curve used by the point source,
        // rather than double-counting the parent's interpolated contribution.
        let interpolated_power: f32 = emitters.iter().map(|e| e.power).sum();
        let target_power = a.power + (b.power - a.power) * t;
        if interpolated_power > 0.0 {
            let correction = target_power / interpolated_power;
            for emitter in &mut emitters {
                emitter.power *= correction;
            }
        }
        if age < 0.0 || f64::from(age) > (self.frames.len() - 1) as f64 * FRAME_DT {
            for emitter in &mut emitters {
                emitter.power = 0.0;
            }
        }
        for emitter in &mut emitters {
            emitter.position *= f64::from(self.length_scale);
            emitter.birth_position *= f64::from(self.length_scale);
            emitter.velocity *= f64::from(self.length_scale);
            emitter.width *= self.length_scale;
        }
        emitters
    }

    pub fn sample(&self, event: &mut BolidGpuEvent, render_origin: [f64; 3]) {
        let origin = DVec3::from_array(render_origin);
        let age = event.start_age[3];
        let seed = event.style[1];
        let emitters = self.sample_emitters(age);
        let power: f64 = emitters.iter().map(|e| f64::from(e.power)).sum();
        let centroid = if power > 0.0 {
            emitters
                .iter()
                .fold(DVec3::ZERO, |sum, e| sum + e.position * f64::from(e.power))
                / power
        } else {
            emitters[0].position
        };
        event.light_position_power = centroid
            .as_vec3()
            .extend(
                // Efficiency is already in power. Convert visible watts into
                // the same scene units as daylight, then isotropic intensity.
                (power * 1e8
                    / (crate::bolid_light::VISIBLE_WATTS_PER_SCENE_UNIT
                        * 4.0
                        * std::f64::consts::PI
                        * 1e10)) as f32
                    * self.length_scale.powi(2),
            )
            .to_array();
        event.head_power = (emitters[0].position - origin)
            .as_vec3()
            .extend(emitters[0].power)
            .to_array();
        let color = if seed < 0.28 {
            Vec3::new(1.0, 0.92, 0.78)
        } else if seed < 0.52 {
            Vec3::new(1.0, 0.77, 0.42)
        } else if seed < 0.76 {
            Vec3::new(0.67, 1.0, 0.73)
        } else {
            Vec3::new(0.65, 0.83, 1.0)
        };
        event.emission = color
            .lerp(Vec3::ONE, emitters[0].temperature * 0.25)
            .extend(emitters[0].power)
            .to_array();
        for (out, emitter) in event.fragments.iter_mut().zip(&emitters[1..]) {
            *out = (emitter.position - origin)
                .as_vec3()
                .extend(emitter.power)
                .to_array();
        }
        let previous = self.sample_emitters(age - FRESH_WAKE_SECONDS);
        for (i, emitter) in emitters.iter().enumerate() {
            let (tail, tail_power) = if previous[i].serial == emitter.serial && age > emitter.born {
                (previous[i].position, previous[i].power)
            } else if age > emitter.born {
                // A short-lived daughter must not wait a whole wake interval
                // for history. Start at its actual breakup/release position,
                // never at the previous occupant of a reused body slot.
                (emitter.birth_position, emitter.birth_power)
            } else {
                (emitter.position, emitter.power)
            };
            event.tails[i] = (tail - origin).as_vec3().extend(tail_power).to_array();
            event.optics[i] = [
                emitter.width,
                emitter.temperature,
                emitter.burst_age,
                // Integer part marks grains; fractional part is wake age.
                f32::from(emitter.grains) + (age - emitter.born).clamp(0.0, FRESH_WAKE_SECONDS),
            ];
        }
        event.end_duration[3] = self.luminous_end().max(0.01);
        for i in 0..event.trail.len() {
            let deposited = (self.knots[i] as f64 * FRAME_DT) as f32;
            let deposited = deposited.min(age.max(0.0));
            let frame_position = f64::from(deposited) / FRAME_DT;
            let frame = (frame_position as usize).min(self.frames.len() - 1);
            let snapshot = &self.frames[frame];
            let next = &self.frames[(frame + 1).min(self.frames.len() - 1)];
            let t = frame_position.fract() as f32;
            let position = snapshot.position.lerp(next.position, f64::from(t));
            let deposited_power = snapshot.power + (next.power - snapshot.power) * t;
            let deposited_mass = snapshot.deposited + (next.deposited - snapshot.deposited) * t;
            let elapsed = (age - deposited).max(0.0);
            let lifetime_fade = ((event.style[2] - elapsed)
                / (event.style[2] * 0.3).clamp(0.5, 5.0))
            .clamp(0.0, 1.0);
            let height = position.length() as f32;
            let shear = (height * 0.00015 + seed * 6.0).sin();
            let wind = self.side * (18.0 + seed * 45.0 + shear * 28.0)
                + self.up * ((height * 0.00009 + seed * 3.0).sin() * 16.0);
            let mut fresh = (-elapsed / 0.45).exp();
            if elapsed < FRESH_WAKE_SECONDS && deposited_power > 0.0 {
                // Fresh emission follows each surviving history slot instead
                // of piling up exclusively on the aggregate centroid. Retired
                // or reused slots leave their emission in the common train.
                let represented: f32 = self
                    .sample_emitters(deposited)
                    .iter()
                    .enumerate()
                    .filter(|(slot, past)| {
                        past.serial == emitters[*slot].serial
                            && (emitters[*slot].power > 1e-6 || event.tails[*slot][3] > 1e-6)
                    })
                    .map(|(_, past)| past.power)
                    .sum();
                let t = ((elapsed - 0.18) / (FRESH_WAKE_SECONDS - 0.18)).clamp(0.0, 1.0);
                let share = FRESH_WAKE_SHARE * (1.0 - t * t * (3.0 - 2.0 * t));
                fresh *= 1.0 - share * (represented / deposited_power).clamp(0.0, 1.0);
            }
            let cooling = 0.14 * (-elapsed / 2.2).exp();
            let chemical = if event.style[2] > 10.0 {
                let high_air =
                    ((height - self.physical_radius - 65000.0) / 20000.0).clamp(0.0, 1.0);
                // Long-lived afterglow has its own slow reservoir; geometric
                // dilution below already reduces its surface brightness.
                0.3 * high_air
                    * (1.0 - (-elapsed / 0.8).exp())
                    * (-elapsed / (event.style[2] * 0.65)).exp()
            } else {
                0.0
            };
            // Expansion conserves approximate integrated line emission.
            let expansion = 1.0 + elapsed * 8.0 / snapshot.width.max(8.0);
            event.trail[i] = ((position + (wind * elapsed).as_dvec3())
                * f64::from(self.length_scale)
                - origin)
                .as_vec3()
                .extend(deposited_power * (fresh + cooling + chemical) / expansion * lifetime_fade)
                .to_array();
            event.trail_meta[i] = [
                deposited,
                (deposited_mass * 40.0).sqrt().min(1.0) * lifetime_fade,
                (snapshot.width.max(8.0) + elapsed * 8.0) * self.length_scale,
                chemical / (fresh + cooling + chemical).max(1e-8),
            ];
        }
        // Retained in the 1824-byte layout; each primitive now projects its own
        // support instead of using a loose sphere around the complete event.
        event.bounds = [0.0; 4];
    }
}
