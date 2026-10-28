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

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed_input_outcome(lanes: usize) -> Vec<(u64, [u32; 3], [u32; 3], bool)> {
        let mut world = World::default();
        let mut inputs = HashMap::new();
        for number in 1..=32 {
            let id = PlayerId::new(number);
            world.join_at(id, format!("Bot{number}"), [number as f32 * 4.0, 0.0, 0.0]);
            inputs.insert(
                id,
                CharacterInput {
                    direction: match number % 4 {
                        0 => [1.0, 0.0],
                        1 => [0.0, 1.0],
                        2 => [-1.0, 0.0],
                        _ => [0.0, -1.0],
                    },
                    jump: number % 3 == 0,
                },
            );
        }
        let executor = Executor::new(lanes);
        for _ in 0..40 {
            step_players(&mut world, &inputs, 0.05, &executor);
        }
        let mut players = world.snapshot();
        players.sort_unstable_by_key(|player| player.id);
        players
            .into_iter()
            .map(|player| {
                (
                    player.id.get(),
                    player.position.map(f32::to_bits),
                    player.velocity.map(f32::to_bits),
                    player.grounded,
                )
            })
            .collect()
    }

    #[test]
    fn fixed_32_player_inputs_have_identical_outcomes_for_lane_counts() {
        let baseline = fixed_input_outcome(1);
        assert_eq!(baseline, fixed_input_outcome(2));
        let auto_lanes = std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1)
            .clamp(1, 64);
        assert_eq!(baseline, fixed_input_outcome(auto_lanes));
    }
}
