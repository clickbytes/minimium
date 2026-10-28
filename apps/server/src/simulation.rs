use crate::executor::Executor;
use minimium_core::{
    ids::PlayerId,
    world::{step_batch, CharacterInput, CollisionQueryStats, World},
};
use std::collections::HashMap;

pub(crate) fn step_players(
    world: &mut World,
    inputs: &HashMap<PlayerId, CharacterInput>,
    delta_seconds: f32,
    executor: &Executor,
) -> CollisionQueryStats {
    world.step_players_with(
        inputs,
        delta_seconds,
        executor.lane_count(),
        |batches, geometry, delta| {
            executor.map(batches, move |batch| step_batch(batch, &geometry, delta))
        },
    )
}
