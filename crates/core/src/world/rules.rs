use super::collision_index::Aabb;
use super::geometry::{
    clip_vertical_motion_to_triangles, highest_triangle_height_for_player, horizontal_overlap,
    intersects_player, is_walkable_triangle, triangle_height_for_player,
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
pub struct CharacterState {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    pub grounded: bool,
}

impl CharacterState {
    pub fn at_spawn(spawn: [f32; 3]) -> Self {
        Self {
            position: spawn,
            ..Self::default()
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
                if !intersects_player(proposed, collider) {
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
                    .all(|other| !intersects_player(stepped, *other))
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
            if intersects_player(proposed, collider) {
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

pub(super) fn finite_input(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

/// Advances a character on the outside of a sphere using radial gravity.
/// The surface is an analytic collision shell; rendered cube-sphere triangles
/// are deliberately visual-only. Other solid objects are not resolved here.
pub fn step_character_on_planet(
    state: &mut CharacterState,
    input: CharacterInput,
    planet: super::regions::CubeSphere,
    delta_seconds: f32,
) {
    if !delta_seconds.is_finite() || delta_seconds <= 0.0 {
        return;
    }
    let (east, up, north) = planet.frame_at(state.position);
    let mut direction = [
        finite_input(input.direction[0]),
        finite_input(input.direction[1]),
    ];
    let squared = direction.iter().map(|x| x * x).sum::<f32>();
    if squared > 1.0 {
        let inverse = squared.sqrt().recip();
        direction = direction.map(|x| x * inverse);
    }
    let tangent = [
        (east[0] * direction[0] + north[0] * direction[1]) * PLAYER_MOVE_SPEED,
        (east[1] * direction[0] + north[1] * direction[1]) * PLAYER_MOVE_SPEED,
        (east[2] * direction[0] + north[2] * direction[1]) * PLAYER_MOVE_SPEED,
    ];
    let mut vertical = if state.grounded {
        if input.jump { PLAYER_JUMP_SPEED } else { 0.0 }
    } else {
        state
            .velocity
            .iter()
            .zip(up)
            .map(|(a, b)| a * b)
            .sum::<f32>()
    };
    vertical = (vertical - GRAVITY * delta_seconds).max(-MAX_FALL_SPEED);
    let next = [
        state.position[0] + (tangent[0] + up[0] * vertical) * delta_seconds,
        state.position[1] + (tangent[1] + up[1] * vertical) * delta_seconds,
        state.position[2] + (tangent[2] + up[2] * vertical) * delta_seconds,
    ];
    let next_up = planet.up_at(next);
    let distance = next
        .iter()
        .zip(planet.center)
        .map(|(a, b)| (a - b) * (a - b))
        .sum::<f32>()
        .sqrt();
    if distance <= planet.radius && vertical <= 0.0 {
        state.position = [
            planet.center[0] + next_up[0] * planet.radius,
            planet.center[1] + next_up[1] * planet.radius,
            planet.center[2] + next_up[2] * planet.radius,
        ];
        let normal_speed = tangent.iter().zip(next_up).map(|(a, b)| a * b).sum::<f32>();
        state.velocity = [
            tangent[0] - normal_speed * next_up[0],
            tangent[1] - normal_speed * next_up[1],
            tangent[2] - normal_speed * next_up[2],
        ];
        state.grounded = true;
    } else {
        state.position = next;
        state.velocity = [
            tangent[0] + next_up[0] * vertical,
            tangent[1] + next_up[1] * vertical,
            tangent[2] + next_up[2] * vertical,
        ];
        state.grounded = false;
    }
}
