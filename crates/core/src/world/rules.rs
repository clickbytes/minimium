use super::collision_index::Aabb;
use super::geometry::{
    clip_vertical_motion_to_triangles, highest_triangle_height_for_player, horizontal_overlap,
    is_walkable_triangle, oriented_capsule_intersects, triangle_height_for_player,
    triangle_intersects_player,
};
use super::map::MapTriangle;
use super::{
    COLLISION_EPSILON, GRAVITY, MAX_FALL_SPEED, MAX_STEP_UP, PLAYER_HEIGHT, PLAYER_JUMP_SPEED,
    PLAYER_MOVE_SPEED, PLAYER_RADIUS,
};
use crate::ids::PlayerId;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Player {
    pub id: PlayerId,
    pub name: Arc<str>,
    pub position: [f32; 3],
    pub position_mm: Option<super::model::Position3Mm>,
    pub velocity: [f32; 3],
    pub grounded: bool,
    pub health: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShotOutcome {
    pub target_id: Option<PlayerId>,
    pub remaining_health: Option<u16>,
    pub killed: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CharacterInput {
    pub direction: [f32; 2],
    pub jump: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum CharacterGravity {
    #[default]
    FixedY,
    Radial {
        center: [f32; 3],
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CharacterState {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub grounded: bool,
    pub gravity: CharacterGravity,
}

impl CharacterState {
    pub fn at_spawn(spawn: [f32; 3]) -> Self {
        Self {
            position: spawn,
            ..Self::default()
        }
    }

    pub fn at_spawn_with_gravity(spawn: [f32; 3], gravity: CharacterGravity) -> Self {
        Self {
            gravity,
            ..Self::at_spawn(spawn)
        }
    }
}

pub fn step_character(
    state: &mut CharacterState,
    input: CharacterInput,
    colliders: &[Aabb],
    surfaces: &[MapTriangle],
    delta_seconds: f32,
) {
    if !delta_seconds.is_finite() || delta_seconds <= 0.0 {
        return;
    }
    if matches!(state.gravity, CharacterGravity::Radial { .. }) {
        step_radial_character(state, input, colliders, surfaces, delta_seconds);
        return;
    }

    let mut direction = [
        finite_input(input.direction[0]),
        finite_input(input.direction[1]),
    ];
    let direction_length_squared = direction[0] * direction[0] + direction[1] * direction[1];
    if direction_length_squared > 1.0 {
        let inverse_length = direction_length_squared.sqrt().recip();
        direction[0] *= inverse_length;
        direction[1] *= inverse_length;
    }
    state.velocity[0] = direction[0] * PLAYER_MOVE_SPEED;
    state.velocity[2] = direction[1] * PLAYER_MOVE_SPEED;

    let should_jump = input.jump && state.grounded;
    state.grounded = false;
    if should_jump {
        state.velocity[1] = PLAYER_JUMP_SPEED;
    }
    state.velocity[1] = (state.velocity[1] - GRAVITY * delta_seconds).max(-MAX_FALL_SPEED);

    let old_y = state.position[1];
    let mut next_y = old_y + state.velocity[1] * delta_seconds;
    if state.velocity[1] <= 0.0 {
        let mut landing_height = None::<f32>;
        for collider in colliders.iter().copied() {
            if horizontal_overlap(state.position, collider)
                && old_y >= collider.max[1] - COLLISION_EPSILON
                && next_y <= collider.max[1]
                && landing_height.is_none_or(|height| collider.max[1] > height)
            {
                landing_height = Some(collider.max[1]);
            }
        }
        for surface in surfaces {
            if let Some(height) = triangle_height_for_player(state.position, *surface) {
                if old_y >= height - COLLISION_EPSILON
                    && next_y <= height
                    && landing_height.is_none_or(|current| height > current)
                {
                    landing_height = Some(height);
                }
            }
        }
        if let Some(height) = landing_height {
            next_y = height;
            state.velocity[1] = 0.0;
            state.grounded = true;
        }
    } else {
        let mut ceiling_height = None::<f32>;
        for collider in colliders.iter().copied() {
            if horizontal_overlap(state.position, collider)
                && old_y + PLAYER_HEIGHT <= collider.min[1] + COLLISION_EPSILON
                && next_y + PLAYER_HEIGHT >= collider.min[1]
                && ceiling_height.is_none_or(|height| collider.min[1] < height)
            {
                ceiling_height = Some(collider.min[1]);
            }
        }
        for surface in surfaces {
            if let Some(height) = triangle_height_for_player(state.position, *surface) {
                if old_y + PLAYER_HEIGHT <= height + COLLISION_EPSILON
                    && next_y + PLAYER_HEIGHT >= height
                    && ceiling_height.is_none_or(|current| height < current)
                {
                    ceiling_height = Some(height);
                }
            }
        }
        if let Some(height) = ceiling_height {
            next_y = height - PLAYER_HEIGHT;
            state.velocity[1] = 0.0;
        }
    }
    if let Some(safe_y) = clip_vertical_motion_to_triangles(state.position, old_y, next_y, surfaces)
    {
        next_y = safe_y;
        state.velocity[1] = 0.0;
        state.grounded = false;
    }
    state.position[1] = next_y;

    for axis in [0usize, 2] {
        let change = state.velocity[axis] * delta_seconds;
        if change == 0.0 {
            continue;
        }
        let mut candidate = state.position[axis] + change;
        let mut proposed = state.position;
        proposed[axis] = candidate;
        let mut stepped_position = None;
        if state.grounded {
            for collider in colliders.iter().copied() {
                if !oriented_capsule_intersects(proposed, [0.0, 1.0, 0.0], collider) {
                    continue;
                }
                let step_up = collider.max[1] - state.position[1];
                if step_up <= 0.0 || step_up > MAX_STEP_UP {
                    continue;
                }
                let mut stepped = proposed;
                stepped[1] = collider.max[1];
                if colliders
                    .iter()
                    .all(|other| !oriented_capsule_intersects(stepped, [0.0, 1.0, 0.0], *other))
                    && surfaces.iter().copied().all(|surface| {
                        is_walkable_triangle(surface)
                            || !triangle_intersects_player(stepped, surface)
                    })
                {
                    stepped_position = Some(stepped);
                    break;
                }
            }
        }
        if let Some(stepped) = stepped_position {
            state.position = stepped;
            state.velocity[1] = 0.0;
            continue;
        }

        for collider in colliders.iter().copied() {
            proposed = state.position;
            proposed[axis] = candidate;
            if oriented_capsule_intersects(proposed, [0.0, 1.0, 0.0], collider) {
                candidate = match (axis, change.is_sign_positive()) {
                    (0, true) => collider.min[0] - PLAYER_RADIUS,
                    (0, false) => collider.max[0] + PLAYER_RADIUS,
                    (2, true) => collider.min[2] - PLAYER_RADIUS,
                    (2, false) => collider.max[2] + PLAYER_RADIUS,
                    _ => candidate,
                };
            }
        }
        for surface in surfaces.iter().copied() {
            if !is_walkable_triangle(surface) {
                proposed = state.position;
                proposed[axis] = candidate;
                if triangle_intersects_player(proposed, surface) {
                    candidate = state.position[axis];
                    break;
                }
            }
        }
        state.position[axis] = candidate;
    }

    if state.grounded {
        if let Some(height) = highest_triangle_height_for_player(state.position, surfaces) {
            let step = height - state.position[1];
            if (-MAX_STEP_UP..=MAX_STEP_UP).contains(&step) {
                state.position[1] = height;
                state.velocity[1] = 0.0;
            }
        }
    }
}

/// Returns the local up direction and a stable tangent basis for a character.
pub fn character_frame(
    gravity: CharacterGravity,
    position: [f32; 3],
) -> ([f32; 3], [f32; 3], [f32; 3]) {
    let up = match gravity {
        CharacterGravity::FixedY => [0.0, 1.0, 0.0],
        CharacterGravity::Radial { center } => {
            normalize(subtract(position, center)).unwrap_or([0.0, 1.0, 0.0])
        }
    };
    let reference = if up[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let projected = subtract(reference, scale(up, dot(reference, up)));
    let forward = normalize(projected).unwrap_or([0.0, 0.0, -1.0]);
    let right = normalize(cross(forward, up)).unwrap_or([1.0, 0.0, 0.0]);
    (up, right, forward)
}

fn step_radial_character(
    state: &mut CharacterState,
    input: CharacterInput,
    colliders: &[Aabb],
    surfaces: &[MapTriangle],
    delta_seconds: f32,
) {
    let mut direction = [
        finite_input(input.direction[0]),
        finite_input(input.direction[1]),
    ];
    let length_squared = direction[0] * direction[0] + direction[1] * direction[1];
    if length_squared > 1.0 {
        let inverse = length_squared.sqrt().recip();
        direction[0] *= inverse;
        direction[1] *= inverse;
    }
    let should_jump = input.jump && state.grounded;
    state.grounded = false;
    let mut vertical_speed = dot(
        state.velocity,
        character_frame(state.gravity, state.position).0,
    );
    if should_jump {
        vertical_speed = PLAYER_JUMP_SPEED;
    }
    let steps = (PLAYER_MOVE_SPEED * delta_seconds / (PLAYER_RADIUS * 0.5))
        .ceil()
        .clamp(1.0, 8.0) as u32;
    let step_seconds = delta_seconds / steps as f32;
    let mut tangent_velocity = [0.0; 3];
    for _ in 0..steps {
        state.grounded = false;
        vertical_speed = (vertical_speed - GRAVITY * step_seconds).max(-MAX_FALL_SPEED);
        let (up, right, forward) = character_frame(state.gravity, state.position);
        tangent_velocity = add(
            scale(right, direction[0] * PLAYER_MOVE_SPEED),
            scale(forward, direction[1] * PLAYER_MOVE_SPEED),
        );
        let old_position = state.position;
        let mut candidate = add(
            add(old_position, scale(tangent_velocity, step_seconds)),
            scale(up, vertical_speed * step_seconds),
        );
        if vertical_speed <= 0.0 {
            if let Some(distance) = nearest_surface_distance(candidate, up, surfaces) {
                if distance >= -MAX_STEP_UP
                    && distance <= PLAYER_MOVE_SPEED * step_seconds + MAX_STEP_UP
                {
                    candidate = add(candidate, scale(up, distance));
                    vertical_speed = 0.0;
                    state.grounded = true;
                }
            }
        }
        let candidate_up = character_frame(state.gravity, candidate).0;
        if colliders
            .iter()
            .copied()
            .any(|collider| oriented_capsule_intersects(candidate, candidate_up, collider))
        {
            candidate = old_position;
            tangent_velocity = [0.0; 3];
        }
        state.position = candidate;
    }
    let up = character_frame(state.gravity, state.position).0;
    state.velocity = add(tangent_velocity, scale(up, vertical_speed));
    if state.grounded {
        state.velocity = tangent_velocity;
    }
}

fn nearest_surface_distance(
    position: [f32; 3],
    up: [f32; 3],
    surfaces: &[MapTriangle],
) -> Option<f32> {
    let reference = if up[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let forward =
        normalize(subtract(reference, scale(up, dot(reference, up)))).unwrap_or([0.0, 0.0, -1.0]);
    let right = normalize(cross(forward, up)).unwrap_or([1.0, 0.0, 0.0]);
    let samples = [
        position,
        add(position, scale(right, PLAYER_RADIUS)),
        add(position, scale(right, -PLAYER_RADIUS)),
        add(position, scale(forward, PLAYER_RADIUS)),
        add(position, scale(forward, -PLAYER_RADIUS)),
    ];
    let mut nearest = None;
    for surface in surfaces {
        let [a, b, c] = surface.vertices;
        let normal = cross(subtract(b, a), subtract(c, a));
        let denominator = dot(normal, up);
        let normal_length = dot(normal, normal).sqrt();
        if !denominator.is_finite()
            || normal_length <= 1.0e-8
            || denominator.abs() < normal_length * 0.5
        {
            continue;
        }
        for sample in samples {
            let distance = dot(normal, subtract(a, sample)) / denominator;
            let point = add(sample, scale(up, distance));
            if point_in_triangle(point, a, b, c)
                && nearest.is_none_or(|current: f32| distance.abs() < current.abs())
            {
                nearest = Some(distance);
            }
        }
    }
    nearest
}

fn point_in_triangle(point: [f32; 3], a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> bool {
    let v0 = subtract(c, a);
    let v1 = subtract(b, a);
    let v2 = subtract(point, a);
    let dot00 = dot(v0, v0);
    let dot01 = dot(v0, v1);
    let dot02 = dot(v0, v2);
    let dot11 = dot(v1, v1);
    let dot12 = dot(v1, v2);
    let denominator = dot00 * dot11 - dot01 * dot01;
    if denominator.abs() <= 1.0e-12 {
        return false;
    }
    let u = (dot11 * dot02 - dot01 * dot12) / denominator;
    let v = (dot00 * dot12 - dot01 * dot02) / denominator;
    u >= -1.0e-5 && v >= -1.0e-5 && u + v <= 1.00001
}

fn dot(left: [f32; 3], right: [f32; 3]) -> f32 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn cross(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}

fn add(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [left[0] + right[0], left[1] + right[1], left[2] + right[2]]
}

fn subtract(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

fn scale(vector: [f32; 3], amount: f32) -> [f32; 3] {
    [vector[0] * amount, vector[1] * amount, vector[2] * amount]
}

fn normalize(vector: [f32; 3]) -> Option<[f32; 3]> {
    let length_squared = dot(vector, vector);
    (length_squared.is_finite() && length_squared > 1.0e-12)
        .then(|| scale(vector, length_squared.sqrt().recip()))
}

pub(super) fn finite_input(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}
