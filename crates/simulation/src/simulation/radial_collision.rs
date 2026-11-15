//! Bounded swept capsule movement against radial-world triangle surfaces.
use minimium_world::world::{geometry::oriented_capsule_intersects_triangle, MapTriangle};

use super::{PLAYER_HEIGHT, PLAYER_RADIUS};

const MAX_SWEEP_STEPS: usize = 32;
const CONTACT_BISECTIONS: usize = 10;
const MAX_SLIDE_PLANES: usize = 4;

pub(super) struct MotionResolution {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub blocked_upward: bool,
}

pub(super) fn sweep_capsule(
    start: [f32; 3],
    up: [f32; 3],
    delta: [f32; 3],
    velocity: [f32; 3],
    surfaces: &[MapTriangle],
) -> MotionResolution {
    let length = dot(delta, delta).sqrt();
    if !length.is_finite() || length <= f32::EPSILON {
        return MotionResolution {
            position: start,
            velocity,
            blocked_upward: false,
        };
    }
    let steps = (length / (PLAYER_RADIUS * 0.4))
        .ceil()
        .clamp(1.0, MAX_SWEEP_STEPS as f32) as usize;
    let piece = scale(delta, 1.0 / steps as f32);
    let mut position = start;
    let mut velocity = velocity;
    let mut blocked_upward = false;
    for _ in 0..steps {
        let candidate = add(position, piece);
        let contacts = contact_planes(candidate, position, up, piece, surfaces);
        if contacts.is_empty() {
            position = candidate;
            continue;
        }
        let mut low = 0.0;
        let mut high = 1.0;
        for _ in 0..CONTACT_BISECTIONS {
            let middle = (low + high) * 0.5;
            let probe = add(position, scale(piece, middle));
            if intersects(probe, up, surfaces) {
                high = middle;
            } else {
                low = middle;
            }
        }
        position = add(position, scale(piece, low));
        let mut slide = scale(piece, 1.0 - low);
        for _ in 0..MAX_SLIDE_PLANES {
            let mut changed = false;
            for normal in contacts.iter().copied() {
                let into = dot(slide, normal);
                if into < -1.0e-6 {
                    slide = subtract(slide, scale(normal, into));
                    changed = true;
                }
                let velocity_into = dot(velocity, normal);
                if velocity_into < -1.0e-6 {
                    blocked_upward |= dot(up, normal) < -0.45 && dot(velocity, up) > 0.0;
                    velocity = subtract(velocity, scale(normal, velocity_into));
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        let slide_candidate = add(position, slide);
        if !intersects(slide_candidate, up, surfaces) {
            position = slide_candidate;
        }
    }
    MotionResolution {
        position,
        velocity,
        blocked_upward,
    }
}

fn intersects(position: [f32; 3], up: [f32; 3], surfaces: &[MapTriangle]) -> bool {
    surfaces
        .iter()
        .copied()
        .any(|surface| oriented_capsule_intersects_triangle(position, up, surface))
}

fn contact_planes(
    candidate: [f32; 3],
    previous: [f32; 3],
    up: [f32; 3],
    movement: [f32; 3],
    surfaces: &[MapTriangle],
) -> Vec<[f32; 3]> {
    let mut planes = Vec::with_capacity(MAX_SLIDE_PLANES);
    for surface in surfaces.iter().copied() {
        if !oriented_capsule_intersects_triangle(candidate, up, surface) {
            continue;
        }
        let [a, b, c] = surface.vertices;
        let Some(mut normal) = normalize(cross(subtract(b, a), subtract(c, a))) else {
            continue;
        };
        let previous_center = add(previous, scale(up, PLAYER_HEIGHT * 0.5));
        let side = dot(subtract(previous_center, a), normal);
        if side < 0.0 || (side.abs() <= 1.0e-5 && dot(movement, normal) > 0.0) {
            normal = scale(normal, -1.0);
        }
        if dot(movement, normal) >= 0.0 {
            continue;
        }
        if planes
            .iter()
            .all(|existing: &[f32; 3]| dot(*existing, normal).abs() < 0.98)
        {
            planes.push(normal);
            if planes.len() == MAX_SLIDE_PLANES {
                break;
            }
        }
    }
    planes
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| a[axis] + b[axis])
}

fn subtract(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| a[axis] - b[axis])
}

fn scale(vector: [f32; 3], amount: f32) -> [f32; 3] {
    vector.map(|value| value * amount)
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a.iter().zip(b).map(|(left, right)| left * right).sum()
}

fn normalize(vector: [f32; 3]) -> Option<[f32; 3]> {
    let length_squared = dot(vector, vector);
    (length_squared.is_finite() && length_squared > 1.0e-12)
        .then(|| scale(vector, length_squared.sqrt().recip()))
}
