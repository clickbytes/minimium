use super::{precise_step, world_state::PlayerStepResult};
use super::{CollisionIndex, SimulationBatch};
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
            .accumulate(batch.collision_query.fill_for_policy(
                collision_index,
                state,
                delta_seconds,
                step.movement_policy,
            ));
        let mut position_mm = step.position_mm;
        let jump = if let Some(position_mm) = &mut position_mm {
            precise_step::step_character_mm_with_water_outcome(
                &mut state,
                position_mm,
                step.input,
                step.movement_policy,
                batch.collision_query.candidates(),
                batch.collision_query.surface_candidates(),
                delta_seconds,
                step.water,
            )
        } else {
            super::movement::step_character_with_water_outcome(
                &mut state,
                step.input,
                step.movement_policy,
                batch.collision_query.candidates(),
                batch.collision_query.surface_candidates(),
                delta_seconds,
                step.water,
            )
        };
        batch.results.push(PlayerStepResult {
            jump,
            id: step.id,
            state,
            position_mm,
        });
    }
    batch
}
