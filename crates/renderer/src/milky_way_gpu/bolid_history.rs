//! Cached observer-independent emission/deposition history.
use super::bolid_dynamics::{BODY_COUNT, Body, Dynamics, Material};
use glam::{DVec3, Vec3};

pub const FRAME_DT: f64 = 0.04;
pub const MAX_FRAMES: usize = 1501;

#[derive(Clone, Copy, Default)]
pub struct Emitter {
    pub position: Vec3,
    pub birth_position: Vec3,
    pub birth_power: f32,
    pub velocity: Vec3,
    pub power: f32,
    pub temperature: f32,
    pub width: f32,
    pub burst_age: f32,
    pub serial: u32,
    pub born: f32,
    pub grains: bool,
}

impl Emitter {
    fn from_body(body: Body) -> Self {
        let vapor_extent = (body.gas / 1e8).max(0.0).sqrt();
        Self {
            position: body.position.as_vec3(),
            birth_position: body.birth_position.as_vec3(),
            birth_power: (body.birth_power / 1e8) as f32,
            velocity: body.velocity.as_vec3(),
            power: (body.power / 1e8) as f32,
            temperature: body.temperature as f32,
            // Cooling gas contracts as emission falls. It is not an implosion.
            width: (4.0 + 20.0 * vapor_extent.min(3.0)) as f32,
            burst_age: (body.age - (body.last_break - body.birth)).max(0.0) as f32,
            serial: body.serial,
            born: body.birth as f32,
            grains: body.multiplicity > 1.0,
        }
    }

    pub fn interpolate(a: Self, b: Self, t: f32, age: f32, end_age: f32) -> Self {
        if a.serial != b.serial {
            // Reused slots are discontinuous. A daughter starts at its actual
            // release, not at the old occupant or the next 40 ms snapshot.
            if age < b.born {
                return Self {
                    position: a.position + a.velocity * (t * FRAME_DT as f32),
                    ..a
                };
            }
            let since_birth = (age - b.born).max(0.0);
            let fraction = (since_birth / (end_age - b.born).max(1e-6)).clamp(0.0, 1.0);
            return Self {
                position: b.birth_position.lerp(b.position, fraction),
                power: b.birth_power + (b.power - b.birth_power) * fraction,
                burst_age: since_birth,
                ..b
            };
        }
        Self {
            position: a.position.lerp(b.position, t),
            velocity: a.velocity.lerp(b.velocity, t),
            power: a.power + (b.power - a.power) * t,
            temperature: a.temperature + (b.temperature - a.temperature) * t,
            width: a.width + (b.width - a.width) * t,
            burst_age: a.burst_age + (b.burst_age - a.burst_age) * t,
            ..a
        }
    }
}

#[derive(Clone)]
pub struct Snapshot {
    pub emitters: [Emitter; BODY_COUNT],
    pub position: Vec3,
    pub power: f32,
    pub deposited: f32,
    pub width: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct EvolutionDiagnostics {
    pub initial_mass_kg: f64,
    pub surviving_mass_kg: f64,
    pub deposited_mass_kg: f64,
    pub fractures: u32,
    pub grain_releases: u32,
    pub luminous_end_s: f32,
    pub emission_episodes: u32,
    pub peak_radiant_power_w: f64,
    /// Age of the strongest cached emission sample (40 ms spacing).
    pub peak_radiant_age_s: f32,
    pub reached_time_budget_while_luminous: bool,
}

/// Refine where trajectory or luminosity changes most. This preserves separated
/// episodes and flare nodes within the fixed shader segment budget.
pub fn trail_knots(frames: &[Snapshot], count: usize) -> Vec<usize> {
    let last = frames
        .iter()
        .rposition(|f| f.power > 1e-6)
        .unwrap_or(1)
        .min(frames.len() - 1);
    let peak = frames.iter().map(|f| f.power).fold(1e-6, f32::max);
    let mut knots = vec![0, last];
    while knots.len() < count {
        let mut best = (0.0, 0);
        for pair in knots.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            for i in a + 1..b {
                let t = (i - a) as f32 / (b - a) as f32;
                let expected_power = frames[a].power + (frames[b].power - frames[a].power) * t;
                let expected_position = frames[a].position.lerp(frames[b].position, t);
                let score = (frames[i].power - expected_power).abs() / peak
                    + (frames[i].position - expected_position).length() / 20000.0;
                if score > best.0 {
                    best = (score, i);
                }
            }
        }
        if best.0 <= 1e-8 {
            knots.push(last);
        } else {
            knots.push(best.1);
        }
        knots.sort_unstable();
    }
    knots
}

pub fn build(
    start: DVec3,
    velocity: DVec3,
    mass: f64,
    material: Material,
    radius: f64,
) -> (Vec<Snapshot>, EvolutionDiagnostics) {
    let mut dynamics = Dynamics::new(start, velocity, mass, material);
    let mut frames = Vec::with_capacity(256);
    let mut quiet = 0.0;
    let mut luminous_end = 0.0;
    let mut position = start.as_vec3();
    for frame in 0..MAX_FRAMES {
        let time = frame as f64 * FRAME_DT;
        let mut deposited = 0.0;
        if frame > 0 {
            for substep in 0..4 {
                dynamics.step(time - FRAME_DT + (substep + 1) as f64 * 0.01, 0.01, radius);
                deposited += dynamics
                    .bodies
                    .iter()
                    .map(|body| body.deposited)
                    .sum::<f64>();
            }
        }
        let power: f64 = dynamics.bodies.iter().map(|body| body.power).sum();
        if power > 1.0 {
            position = (dynamics
                .bodies
                .iter()
                .fold(DVec3::ZERO, |sum, body| sum + body.position * body.power)
                / power)
                .as_vec3();
        }
        let emitters = dynamics.bodies.map(Emitter::from_body);
        let width = emitters.iter().map(|e| e.width).fold(0.0, f32::max);
        frames.push(Snapshot {
            emitters,
            position,
            power: (power / 1e8) as f32,
            deposited: deposited as f32,
            width,
        });
        if power > 100.0 {
            luminous_end = time;
            quiet = 0.0;
        } else {
            quiet += FRAME_DT;
        }
        // Stop after natural extinction, or after a non-luminous grazing exit.
        // Launch's cold interval must not be mistaken for the end of a meteor.
        if luminous_end > 0.0 && quiet >= 1.0 {
            break;
        }
    }
    let surviving_mass = dynamics.bodies.iter().map(|b| b.mass + b.shed).sum();
    let peak = frames.iter().map(|f| f.power).fold(0.0, f32::max);
    let peak_frame = frames.iter().position(|f| f.power == peak).unwrap_or(0);
    let mut episodes = 0;
    let mut emitting = false;
    for frame in &frames {
        if !emitting && frame.power > peak * 0.12 {
            episodes += 1;
            emitting = true;
        }
        if frame.power < peak * 0.035 {
            emitting = false;
        }
    }
    let diagnostics = EvolutionDiagnostics {
        initial_mass_kg: mass,
        surviving_mass_kg: surviving_mass,
        deposited_mass_kg: dynamics.vaporized + dynamics.unresolved_grains,
        fractures: dynamics.fractures,
        grain_releases: dynamics.grain_releases,
        luminous_end_s: luminous_end as f32,
        emission_episodes: episodes,
        peak_radiant_power_w: f64::from(peak) * 1e8,
        peak_radiant_age_s: (peak_frame as f64 * FRAME_DT) as f32,
        reached_time_budget_while_luminous: frames.len() == MAX_FRAMES && quiet < 1.0,
    };
    (frames, diagnostics)
}
