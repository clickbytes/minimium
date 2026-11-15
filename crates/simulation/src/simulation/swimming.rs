use super::{CharacterGravity, CharacterState, MovementIntent};
use minimium_world::fluid::WaterSample;

const BUOYANCY_M_S2: f32 = 14.0;
const WATER_DRAG_PER_SECOND: f32 = 3.5;
const SWIM_SPEED: f32 = 3.2;
const SWIM_ACCEL: f32 = 12.0;

/// Steering is relative to the surrounding water, so normal walking control
/// cannot cancel the current again at the start of every simulation tick.
pub(super) fn movement_velocity(
    mut state: CharacterState,
    input: MovementIntent,
    mut policy: super::MovementPolicy,
    dt: f32,
    water: Option<WaterSample>,
) -> [f32; 3] {
    let Some(water) = water else {
        return super::movement::velocity(state, input, policy, dt);
    };
    let fraction = water.submerged_fraction as f32;
    let up = super::character_frame(state.gravity, state.position).0;
    let flow = water.velocity_m_s.map(|value| value as f32 * fraction);
    let radial_flow = flow.iter().zip(up).map(|(v, u)| v * u).sum::<f32>();
    let tangent_flow: [f32; 3] = std::array::from_fn(|axis| flow[axis] - up[axis] * radial_flow);
    for axis in 0..3 {
        state.velocity[axis] -= tangent_flow[axis];
    }
    policy.speed += (policy.speed.min(SWIM_SPEED) - policy.speed) * fraction;
    policy.accel += (policy.accel.min(SWIM_ACCEL) - policy.accel) * fraction;
    let velocity = super::movement::velocity(state, input, policy, dt);
    std::array::from_fn(|axis| velocity[axis] + tangent_flow[axis])
}

/// Applies water forces after gravity/steering and before collision movement.
pub(super) fn apply_swimming(
    state: &mut CharacterState,
    input: MovementIntent,
    water: WaterSample,
    dt: f32,
) {
    let fraction = water.submerged_fraction as f32;
    if fraction <= 0.0 || !dt.is_finite() || dt <= 0.0 {
        return;
    }
    let up = match state.gravity {
        CharacterGravity::FixedY => [0.0, 1.0, 0.0],
        _ => super::character_frame(state.gravity, state.position).0,
    };
    let flow: [f32; 3] = water.velocity_m_s.map(|v| v as f32);
    let blend = (WATER_DRAG_PER_SECOND * fraction * dt.min(0.2)).clamp(0.0, 0.7);
    for axis in 0..3 {
        state.velocity[axis] += (flow[axis] - state.velocity[axis]) * blend;
        state.velocity[axis] += up[axis] * (BUOYANCY_M_S2 * fraction * dt.min(0.2));
    }
    let swim = input.vertical.clamp(-1.0, 1.0);
    if swim.abs() <= f32::EPSILON {
        return;
    }
    let vertical_speed = state
        .velocity
        .iter()
        .zip(up)
        .map(|(v, u)| v * u)
        .sum::<f32>();
    let radial_flow = flow.iter().zip(up).map(|(v, u)| v * u).sum::<f32>();
    let target = radial_flow + swim * SWIM_SPEED;
    let acceleration = SWIM_ACCEL * fraction * dt;
    let correction = (target - vertical_speed).clamp(-acceleration, acceleration);
    for axis in 0..3 {
        state.velocity[axis] += up[axis] * correction;
    }
}
