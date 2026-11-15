//! SI atmospheric integration with continuous terrain contact. No I/O or generation.
use minimium_world::{
    impact::BodyState,
    terrain_edits::{length, project},
};

pub trait BodyEnvironment {
    fn can_fragment(&self) -> bool {
        true
    }
    /// Radius of solid ground at this direction, in game-world metres.
    fn ground_radius(&self, direction: [f64; 3]) -> Option<f64>;
    /// Signed distance from a game-world point to current solid terrain. The
    /// default preserves the generated spherical surface for existing hosts.
    fn terrain_distance(&self, position: [f64; 3]) -> Option<f64> {
        let radius = self.ground_radius(position)?;
        Some(length(position) - radius)
    }
    /// Sweeps the body's center through current terrain with a spherical radius.
    fn first_surface_contact(
        &self,
        start: [f64; 3],
        end: [f64; 3],
        clearance: f64,
    ) -> SegmentSurfaceContact {
        segment_contact(self, start, end, clearance)
    }
    fn air_density(&self, physical_altitude_m: f64) -> f64 {
        1.225 * (-physical_altitude_m.max(0.0) / 7800.0).exp()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SegmentSurfaceContact {
    Hit([f64; 3]),
    Clear,
    Unavailable,
}

#[derive(Default)]
pub struct BodyStep {
    pub contacted: bool,
    pub fragment: Option<BodyState>,
    pub awaiting_surface: bool,
}

pub fn step_body(body: &mut BodyState, dt: f64, environment: &dyn BodyEnvironment) -> BodyStep {
    let mut result = BodyStep::default();
    if body.phase != 0 || !dt.is_finite() || dt <= 0.0 {
        return result;
    }
    let speed = length(body.velocity);
    // At most 10 game metres per sweep, bounded independently of frame rate.
    let steps = ((speed * body.metres_scale * dt / 10.0).ceil() as usize).clamp(1, 256);
    let h = dt.min(0.1) / steps as f64;
    for _ in 0..steps {
        let Some(_) = environment.ground_radius(body.position) else {
            result.awaiting_surface = true;
            break;
        };
        let altitude =
            ((length(body.position) - body.planet_radius_m) / body.metres_scale).max(0.0);
        let speed = length(body.velocity);
        let density = environment.air_density(altitude);
        let area = std::f64::consts::PI * body.radius_m().powi(2);
        let drag = (0.5 * density * area * speed * 0.8 / body.mass_kg * h).min(0.2);
        let up = project(body.position, 1.0);
        let before = body.position;
        for i in 0..3 {
            body.velocity[i] = body.velocity[i] * (1.0 - drag) - up[i] * 9.81 * h;
            body.position[i] += body.velocity[i] * h * body.metres_scale;
        }
        // Energy consumed by ablation; retain a finite remnant for dark flight.
        let loss =
            (0.5 * density * area * speed.powi(3) * 0.006 / 8e6 * h).min(body.mass_kg * 0.02);
        body.mass_kg = (body.mass_kg - loss).max(body.initial_mass_kg * 0.001);
        body.age_s += h;
        if body.fragmentation == 0
            && environment.can_fragment()
            && 0.5 * density * speed * speed > 240000.0
        {
            body.fragmentation = 1;
            let mut fragment = *body;
            let fraction = 0.12 + (body.seed % 11) as f64 * 0.005;
            fragment.mass_kg = body.mass_kg * fraction;
            fragment.initial_mass_kg = fragment.mass_kg;
            fragment.fragmentation = 3;
            fragment.crater_radius_m = (body.crater_radius_m * 0.45).max(6.0);
            let axis = if up[1].abs() < 0.9 {
                [0.0, 1.0, 0.0]
            } else {
                [1.0, 0.0, 0.0]
            };
            let side = project(
                [
                    up[1] * axis[2] - up[2] * axis[1],
                    up[2] * axis[0] - up[0] * axis[2],
                    up[0] * axis[1] - up[1] * axis[0],
                ],
                45.0,
            );
            body.mass_kg -= fragment.mass_kg;
            for i in 0..3 {
                fragment.velocity[i] += side[i];
                body.velocity[i] -= side[i] * fragment.mass_kg / body.mass_kg;
            }
            result.fragment = Some(fragment);
        }
        let collision_radius = body.radius_m() * body.metres_scale;
        match environment.first_surface_contact(before, body.position, collision_radius) {
            SegmentSurfaceContact::Hit(position) => {
                body.position = position;
                body.energy_j = 0.5 * body.mass_kg * length(body.velocity).powi(2);
                // Small energy produces a smaller, still resolvable surface depression.
                let energy_scale = (body.energy_j / 1e10).powf(0.15).clamp(0.3, 1.0);
                body.crater_radius_m = (body.crater_radius_m * energy_scale).max(6.0);
                body.velocity = [0.0; 3];
                body.phase = 1;
                result.contacted = true;
                break;
            }
            SegmentSurfaceContact::Unavailable => {
                body.position = before;
                result.awaiting_surface = true;
                break;
            }
            SegmentSurfaceContact::Clear => {}
        }
    }
    result
}

fn segment_contact<E: BodyEnvironment + ?Sized>(
    environment: &E,
    start: [f64; 3],
    end: [f64; 3],
    clearance: f64,
) -> SegmentSurfaceContact {
    if !clearance.is_finite() || clearance < 0.0 {
        return SegmentSurfaceContact::Unavailable;
    }
    let delta = std::array::from_fn::<_, 3, _>(|i| end[i] - start[i]);
    let distance = length(delta);
    let point = |t: f64| std::array::from_fn(|i| start[i] + delta[i] * t);
    let Some(mut signed) = environment.terrain_distance(start) else {
        return SegmentSurfaceContact::Unavailable;
    };
    signed -= clearance;
    if signed <= 0.0 {
        return SegmentSurfaceContact::Hit(start);
    }
    if signed > distance {
        return SegmentSurfaceContact::Clear;
    }
    let mut previous_t = 0.0;
    for _ in 0..128 {
        let remaining = distance * (1.0 - previous_t);
        // Half-distance advances tolerate moderate field-gradient error while
        // still crossing kilometre-scale atmospheric segments in few samples.
        let advance = signed * 0.5;
        if advance <= 1.0e-4 {
            return SegmentSurfaceContact::Hit(point(previous_t));
        }
        if advance >= remaining {
            return SegmentSurfaceContact::Clear;
        }
        let t = previous_t + advance / distance;
        let Some(current) = environment.terrain_distance(point(t)) else {
            return SegmentSurfaceContact::Unavailable;
        };
        let current = current - clearance;
        if current <= 0.0 {
            let (mut lo, mut hi) = (previous_t, t);
            for _ in 0..20 {
                let mid = (lo + hi) * 0.5;
                let Some(distance) = environment.terrain_distance(point(mid)) else {
                    return SegmentSurfaceContact::Unavailable;
                };
                if distance > clearance {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            return SegmentSurfaceContact::Hit(point(hi));
        }
        previous_t = t;
        signed = current;
    }
    // Never turn an exhausted query budget into a missed collision.
    SegmentSurfaceContact::Unavailable
}
