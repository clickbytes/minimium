use super::*;

impl World {
    pub(super) fn prepare_movement_batches(
        &mut self,
        inputs: &HashMap<PlayerId, MovementIntent>,
        selected: Option<&HashSet<PlayerId>>,
        lanes: usize,
    ) -> Option<(Arc<CollisionIndex>, Vec<SimulationBatch>)> {
        let selected_count = self
            .players
            .keys()
            .filter(|id| selected.is_none_or(|ids| ids.contains(*id)))
            .count();
        if lanes <= 1 || selected_count < PARALLEL_SIMULATION_MIN_PLAYERS {
            return None;
        }
        let players = Arc::make_mut(&mut self.players);
        let movement_policy = self.movement_policy;
        let player_movement_policies = Arc::clone(&self.player_movement_policies);
        // Preserve the original copy-on-write timing before worker execution.
        Arc::make_mut(&mut self.cells);
        Arc::make_mut(&mut self.player_cells);
        let simulation_batches = &mut self.simulation_batches;
        let batch_count = lanes
            .min(selected_count.div_ceil(PLAYERS_PER_SIMULATION_BATCH))
            .max(2);
        simulation_batches.resize_with(batch_count, SimulationBatch::default);
        let inputs_per_batch = selected_count.div_ceil(batch_count);
        for batch in simulation_batches.iter_mut() {
            batch.inputs.clear();
            batch.results.clear();
            batch.collision_stats = CollisionQueryStats::default();
            if batch.inputs.capacity() < inputs_per_batch {
                batch.inputs.reserve(inputs_per_batch);
            }
            if batch.results.capacity() < inputs_per_batch {
                batch.results.reserve(inputs_per_batch);
            }
        }
        for (index, (id, player)) in players
            .iter()
            .filter(|(id, _)| selected.is_none_or(|ids| ids.contains(*id)))
            .enumerate()
        {
            let batch_index = (index / inputs_per_batch).min(batch_count - 1);
            simulation_batches[batch_index]
                .inputs
                .push(PlayerStepInput {
                    id: *id,
                    state: CharacterState {
                        position: player.position,
                        velocity: player.velocity,
                        grounded: player.grounded,
                        gravity: self.gravity,
                    },
                    position_mm: player.position_mm,
                    input: inputs.get(id).copied().unwrap_or_default(),
                    movement_policy: player_movement_policies
                        .get(id)
                        .copied()
                        .unwrap_or(movement_policy),
                });
        }

        Some((
            Arc::clone(&self.collision_index),
            std::mem::take(simulation_batches),
        ))
    }
}
