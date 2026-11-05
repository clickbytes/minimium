use super::{precise_step, world_state::PlayerStepResult};
use super::{step_character, CollisionIndex, SimulationBatch};
/// Advances a batch using immutable collision geometry; it never mutates a world.
pub fn step_batch(
    mut batch: SimulationBatch,
    collision_index: &CollisionIndex,
    delta_seconds: f32,
) -> SimulationBatch {
    for step in batch.inputs.drain(..) {
        let mut state = step.state;
        batch
            .collision_stats
            .accumulate(batch.collision_query.fill_for_speed(
                collision_index,
                state,
                delta_seconds,
                step.input.movement.speed,
            ));
        let mut position_mm = step.position_mm;
        if let Some(position_mm) = &mut position_mm {
            precise_step::step_character_mm(
                &mut state,
                position_mm,
                step.input,
                batch.collision_query.candidates(),
                batch.collision_query.surface_candidates(),
                delta_seconds,
            );
        } else {
            step_character(
                &mut state,
                step.input,
                batch.collision_query.candidates(),
                batch.collision_query.surface_candidates(),
                delta_seconds,
            );
        }
        batch.results.push(PlayerStepResult {
            id: step.id,
            state,
            position_mm,
        });
    }
    batch
}
