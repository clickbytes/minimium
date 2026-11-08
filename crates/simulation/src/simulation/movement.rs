use super::{character_frame, CharacterState, MovementIntent};
use minimium_world::world::MapTriangle;

pub const MOVEMENT_LIMIT: f32 = 10_000.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovementPolicy {
    pub can_fly: bool,
    pub speed: f32,
    pub accel: f32,
    pub revision: u64,
}

impl Default for MovementPolicy {
    fn default() -> Self {
        Self {
            can_fly: false,
            speed: 5.0,
            accel: 30.0,
            revision: 1,
        }
    }
}

impl MovementPolicy {
    pub fn valid(self) -> bool {
        self.revision != 0
            && [self.speed, self.accel]
                .into_iter()
                .all(|v| v.is_finite() && (0.0..=MOVEMENT_LIMIT).contains(&v))
    }
}

pub(super) fn velocity(
    state: CharacterState,
    input: MovementIntent,
    policy: MovementPolicy,
    dt: f32,
) -> [f32; 3] {
    let (up, right, forward) = match state.gravity {
        super::CharacterGravity::FixedY => ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        _ => character_frame(state.gravity, state.position),
    };
    let vertical = if policy.can_fly { input.vertical } else { 0.0 };
    let length = (input.direction[0].powi(2) + input.direction[1].powi(2) + vertical.powi(2))
        .sqrt()
        .max(1.0);
    let target: [f32; 3] = std::array::from_fn(|i| {
        (right[i] * input.direction[0] + forward[i] * input.direction[1] + up[i] * vertical)
            * policy.speed
            / length
    });
    let radial = state
        .velocity
        .iter()
        .zip(up)
        .map(|(a, b)| a * b)
        .sum::<f32>();
    let previous: [f32; 3] = std::array::from_fn(|i| {
        state.velocity[i] - if policy.can_fly { 0.0 } else { up[i] * radial }
    });
    let delta: [f32; 3] = std::array::from_fn(|i| target[i] - previous[i]);
    let distance = delta.iter().map(|v| v * v).sum::<f32>().sqrt();
    let factor = if distance > 0.0 {
        (policy.accel * dt / distance).min(1.0)
    } else {
        0.0
    };
    let mut result: [f32; 3] = std::array::from_fn(|i| previous[i] + delta[i] * factor);
    let speed = result.iter().map(|v| v * v).sum::<f32>().sqrt();
    if speed > policy.speed {
        result = result.map(|v| v * policy.speed / speed);
    }
    result
}

/// One shared bounded integrator for authoritative movement and prediction.
pub fn step_character(
    state: &mut CharacterState,
    mut input: MovementIntent,
    policy: MovementPolicy,
    colliders: &[super::Aabb],
    surfaces: &[MapTriangle],
    dt: f32,
) {
    if !dt.is_finite() || dt <= 0.0 || !policy.valid() {
        return;
    }
    input.direction = input.direction.map(super::rules::finite_input);
    input.vertical = super::rules::finite_input(input.vertical);
    let dt = dt.min(super::SIMULATION_STEP_SECONDS);
    let steps = if policy.can_fly {
        1
    } else {
        (policy.speed * dt / (super::PLAYER_RADIUS * 0.5))
            .ceil()
            .max(1.0) as u32
    };
    for index in 0..steps {
        super::rules::step_character_once(
            state,
            input,
            policy,
            colliders,
            surfaces,
            dt / steps as f32,
        );
        if index == 0 {
            input.jump = false;
        }
    }
}
