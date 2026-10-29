use super::collision_index::{Aabb, Cell, CollisionIndex};
use super::collision_queries::{CollisionQuery, CollisionQueryStats};
use super::geometry::{cell_for, intersects_player, update_cell_index};
use super::map::{MapTriangle, WorldMap};
use super::raycast::{MapRayQuery, PlayerRayQuery};
use super::rules::{step_character, CharacterGravity, CharacterInput, CharacterState, Player};
use super::snapshot::WorldSnapshot;
use super::{
    MAX_PLAYER_NAME_CHARS, PARALLEL_SIMULATION_MIN_PLAYERS, PLAYERS_PER_SIMULATION_BATCH,
    PLAYER_MAX_HEALTH,
};
use crate::ids::PlayerId;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Clone, Copy)]
pub(super) struct PlayerStepInput {
    pub(super) id: PlayerId,
    pub(super) state: CharacterState,
    pub(super) input: CharacterInput,
}

#[derive(Clone, Copy)]
pub(super) struct PlayerStepResult {
    pub(super) id: PlayerId,
    pub(super) state: CharacterState,
}

#[derive(Default)]
pub struct SimulationBatch {
    pub(super) inputs: Vec<PlayerStepInput>,
    pub(super) results: Vec<PlayerStepResult>,
    pub(super) collision_query: CollisionQuery,
    pub(super) collision_stats: CollisionQueryStats,
}

#[derive(Default)]
pub struct World {
    pub(super) players: Arc<HashMap<PlayerId, Player>>,
    pub(super) cells: Arc<HashMap<Cell, Vec<PlayerId>>>,
    pub(super) player_cells: Arc<HashMap<PlayerId, Cell>>,
    pub(super) collision_index: Arc<CollisionIndex>,
    pub(super) combat_index: Arc<CollisionIndex>,
    pub(super) collision_query: CollisionQuery,
    pub(super) combat_player_query: PlayerRayQuery,
    pub(super) combat_map_query: MapRayQuery,
    pub(super) simulation_batches: Vec<SimulationBatch>,
    pub(super) next_id: u64,
    pub(super) spawn: [f32; 3],
    pub(super) gravity: CharacterGravity,
}

impl World {
    pub fn join(&mut self, name: String) -> PlayerId {
        self.next_id += 1;
        let id = PlayerId::new(self.next_id);
        self.join_as(id, name);
        id
    }

    pub fn join_as(&mut self, id: PlayerId, name: String) {
        self.join_at(id, name, self.spawn);
    }

    pub fn join_at(&mut self, id: PlayerId, name: String, position: [f32; 3]) {
        self.join_at_with_health(id, name, position, PLAYER_MAX_HEALTH);
    }

    pub fn join_at_with_health(
        &mut self,
        id: PlayerId,
        name: String,
        position: [f32; 3],
        health: u16,
    ) {
        // Replacing an existing ID must first remove its old spatial-cell membership.
        // Otherwise one player may remain indexed in multiple cells.
        if self.players.contains_key(&id) {
            self.leave(id);
        }
        self.next_id = self.next_id.max(id.get());
        let position = if position.iter().all(|value| value.is_finite())
            && self
                .collision_index
                .colliders
                .iter()
                .all(|collider| !intersects_player(position, *collider))
        {
            position
        } else {
            self.spawn
        };
        let player = Player {
            id,
            name: Arc::from(name),
            position,
            velocity: [0.0; 3],
            grounded: false,
            health: health.clamp(1, PLAYER_MAX_HEALTH),
        };
        let cell = cell_for(player.position);
        Arc::make_mut(&mut self.cells)
            .entry(cell)
            .or_default()
            .push(player.id);
        Arc::make_mut(&mut self.player_cells).insert(player.id, cell);
        Arc::make_mut(&mut self.players).insert(id, player);
    }

    pub fn leave(&mut self, id: PlayerId) {
        Arc::make_mut(&mut self.players).remove(&id);
        if let Some(cell) = Arc::make_mut(&mut self.player_cells).remove(&id) {
            let cells = Arc::make_mut(&mut self.cells);
            let empty = if let Some(players) = cells.get_mut(&cell) {
                players.retain(|player| *player != id);
                players.is_empty()
            } else {
                false
            };
            if empty {
                cells.remove(&cell);
            }
        }
    }

    pub fn step_players(
        &mut self,
        inputs: &HashMap<PlayerId, CharacterInput>,
        delta_seconds: f32,
    ) -> CollisionQueryStats {
        self.step_players_selected(inputs, None, delta_seconds)
    }

    /// Advances only players whose required collision sectors are installed.
    /// Other players retain position and velocity until their geometry is ready.
    pub fn step_players_selected(
        &mut self,
        inputs: &HashMap<PlayerId, CharacterInput>,
        selected: Option<&HashSet<PlayerId>>,
        delta_seconds: f32,
    ) -> CollisionQueryStats {
        let Self {
            players,
            cells,
            player_cells,
            collision_index,
            collision_query,
            gravity,
            ..
        } = self;
        let players = Arc::make_mut(players);
        let cells = Arc::make_mut(cells);
        let player_cells = Arc::make_mut(player_cells);
        let mut collision_stats = CollisionQueryStats::default();
        for (id, player) in players {
            if selected.is_some_and(|ids| !ids.contains(id)) {
                continue;
            }
            let mut state = CharacterState {
                position: player.position,
                velocity: player.velocity,
                grounded: player.grounded,
                gravity: *gravity,
            };
            let input = inputs.get(id).copied().unwrap_or_default();
            collision_stats.accumulate(collision_query.fill(collision_index, state, delta_seconds));
            step_character(
                &mut state,
                input,
                collision_query.candidates(),
                collision_query.surface_candidates(),
                delta_seconds,
            );
            player.position = state.position;
            player.velocity = state.velocity;
            player.grounded = state.grounded;
            update_cell_index(cells, player_cells, *id, player.position);
        }
        collision_stats
    }

    /// Runs movement on the authoritative owner, delegating only independent batches.
    /// The dispatcher must return every processed batch in input order.
    pub fn step_players_with(
        &mut self,
        inputs: &HashMap<PlayerId, CharacterInput>,
        delta_seconds: f32,
        lanes: usize,
        dispatch: impl FnOnce(Vec<SimulationBatch>, Arc<CollisionIndex>, f32) -> Vec<SimulationBatch>,
    ) -> CollisionQueryStats {
        self.step_players_with_selected(inputs, None, delta_seconds, lanes, dispatch)
    }

    /// Runs the selected players on the owner lane or in immutable worker batches.
    pub fn step_players_with_selected(
        &mut self,
        inputs: &HashMap<PlayerId, CharacterInput>,
        selected: Option<&HashSet<PlayerId>>,
        delta_seconds: f32,
        lanes: usize,
        dispatch: impl FnOnce(Vec<SimulationBatch>, Arc<CollisionIndex>, f32) -> Vec<SimulationBatch>,
    ) -> CollisionQueryStats {
        let Some((geometry, batches)) = self.prepare_movement_batches(inputs, selected, lanes)
        else {
            return self.step_players_selected(inputs, selected, delta_seconds);
        };
        let results = dispatch(batches, geometry, delta_seconds);
        self.apply_step_results(results)
    }

    fn prepare_movement_batches(
        &mut self,
        inputs: &HashMap<PlayerId, CharacterInput>,
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
                    input: inputs.get(id).copied().unwrap_or_default(),
                });
        }

        Some((
            Arc::clone(&self.collision_index),
            std::mem::take(simulation_batches),
        ))
    }

    fn apply_step_results(
        &mut self,
        processed_batches: Vec<SimulationBatch>,
    ) -> CollisionQueryStats {
        let players = Arc::make_mut(&mut self.players);
        let cells = Arc::make_mut(&mut self.cells);
        let player_cells = Arc::make_mut(&mut self.player_cells);
        let mut collision_stats = CollisionQueryStats::default();
        for batch in &processed_batches {
            collision_stats.accumulate(batch.collision_stats);
            for result in &batch.results {
                if let Some(player) = players.get_mut(&result.id) {
                    player.position = result.state.position;
                    player.velocity = result.state.velocity;
                    player.grounded = result.state.grounded;
                    update_cell_index(cells, player_cells, result.id, player.position);
                }
            }
        }
        self.simulation_batches = processed_batches;
        collision_stats
    }

    pub fn set_position(&mut self, id: PlayerId, position: [f32; 3]) -> bool {
        if !position.iter().all(|value| value.is_finite()) {
            return false;
        }
        let Some(player) = Arc::make_mut(&mut self.players).get_mut(&id) else {
            return false;
        };
        player.position = position;
        player.velocity = [0.0; 3];
        player.grounded = false;
        self.update_player_cell(id, position);
        true
    }

    pub fn player_health(&self, id: PlayerId) -> Option<u16> {
        self.players.get(&id).map(|player| player.health)
    }

    pub fn player(&self, id: PlayerId) -> Option<&Player> {
        self.players.get(&id)
    }

    fn update_player_cell(&mut self, id: PlayerId, position: [f32; 3]) {
        update_cell_index(
            Arc::make_mut(&mut self.cells),
            Arc::make_mut(&mut self.player_cells),
            id,
            position,
        );
    }

    pub fn rename_player(&mut self, id: PlayerId, name: String) -> bool {
        let Some(player) = Arc::make_mut(&mut self.players).get_mut(&id) else {
            return false;
        };
        player.name = Arc::from(name.chars().take(MAX_PLAYER_NAME_CHARS).collect::<String>());
        true
    }

    pub fn snapshot(&self) -> Vec<Player> {
        self.players.values().cloned().collect()
    }

    pub fn network_snapshot(&self) -> WorldSnapshot {
        WorldSnapshot {
            players: Arc::clone(&self.players),
            cells: Arc::clone(&self.cells),
            gravity: self.gravity,
        }
    }

    pub fn player_count(&self) -> usize {
        self.players.len()
    }

    pub fn gravity(&self) -> CharacterGravity {
        self.gravity
    }

    pub fn add_collider(&mut self, collider: Aabb) {
        Arc::make_mut(&mut self.collision_index).add(collider);
        Arc::make_mut(&mut self.combat_index).add(collider);
    }

    pub fn load_map(&mut self, map: &WorldMap) {
        self.spawn = map.spawn;
        let index = Arc::new(map.collision_index());
        self.collision_index = Arc::clone(&index);
        self.combat_index = index;
        self.reset_collision_queries();
    }

    /// Loads map metadata while keeping movement collision dependent on the
    /// sectors installed by the owning server.
    pub fn load_map_streamed(&mut self, map: &WorldMap) {
        self.spawn = map.spawn;
        self.collision_index = Arc::new(CollisionIndex::default());
        self.combat_index = Arc::new(map.collision_index());
        self.reset_collision_queries();
    }

    pub fn replace_streamed_collision(&mut self, colliders: Vec<Aabb>, surfaces: Vec<MapTriangle>) {
        self.collision_index = Arc::new(CollisionIndex::from_geometry(colliders, surfaces));
        self.collision_query = CollisionQuery::default();
        self.simulation_batches.clear();
    }

    fn reset_collision_queries(&mut self) {
        self.collision_query = CollisionQuery::default();
        self.combat_player_query = PlayerRayQuery::default();
        self.combat_map_query = MapRayQuery::default();
        self.simulation_batches.clear();
    }

    pub fn set_gravity(&mut self, gravity: CharacterGravity) {
        self.gravity = gravity;
    }
}

/// Advances a batch using immutable collision geometry; it never mutates a world.
pub fn step_batch(
    mut batch: SimulationBatch,
    collision_index: &CollisionIndex,
    delta_seconds: f32,
) -> SimulationBatch {
    for step in batch.inputs.drain(..) {
        let mut state = step.state;
        batch.collision_stats.accumulate(batch.collision_query.fill(
            collision_index,
            state,
            delta_seconds,
        ));
        step_character(
            &mut state,
            step.input,
            batch.collision_query.candidates(),
            batch.collision_query.surface_candidates(),
            delta_seconds,
        );
        batch.results.push(PlayerStepResult { id: step.id, state });
    }
    batch
}
