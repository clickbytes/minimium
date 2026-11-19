mod geometry;
mod movement;

use super::collision_index::CollisionIndex;
use super::collision_queries::{CollisionQuery, CollisionQueryStats};
use super::precise_step;
use super::raycast::{MapRayQuery, PlayerRayQuery};
use super::rules::{CharacterState, MovementIntent, Player};
use super::snapshot::WorldSnapshot;
use super::spatial::update_cell_index;
use super::spatial_geometry::SpatialGeometryPublication;
use super::{
    MAX_PLAYER_NAME_CHARS, PARALLEL_SIMULATION_MIN_PLAYERS, PLAYERS_PER_SIMULATION_BATCH,
    PLAYER_MAX_HEALTH,
};
use minimium_core::ids::PlayerId;
use minimium_world::world::geometry::{intersects_player, oriented_capsule_intersects};
use minimium_world::world::{
    cell_for, character_frame, Cell, CharacterGeometry, CharacterGravity, Position3Mm,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq)]
pub(super) struct PlayerStepInput {
    pub(super) id: PlayerId,
    pub(super) state: CharacterState,
    pub(super) position_mm: Option<Position3Mm>,
    pub(super) input: MovementIntent,
    pub(super) movement_policy: super::MovementPolicy,
    pub(super) water: Option<minimium_world::fluid::WaterSample>,
}

#[derive(Clone, Copy)]
pub(super) struct PlayerStepResult {
    pub(super) jump: Option<super::JumpResolution>,
    pub(super) id: PlayerId,
    pub(super) state: CharacterState,
    pub(super) position_mm: Option<Position3Mm>,
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
    pub(super) spatial_geometry: Arc<SpatialGeometryPublication>,
    pub(super) collision_query: CollisionQuery,
    pub(super) combat_player_query: PlayerRayQuery,
    pub(super) combat_map_query: MapRayQuery,
    pub(super) jump_resolutions: Vec<(PlayerId, super::JumpResolution)>,
    pub(super) simulation_batches: Vec<SimulationBatch>,
    pub(super) next_id: u64,
    pub(super) spawn: [f32; 3],
    pub(super) spawn_mm: Option<Position3Mm>,
    pub(super) gravity: CharacterGravity,
    pub(super) precise_positions: bool,
    pub(super) movement_policy: super::MovementPolicy,
    pub(super) player_movement_policies: Arc<HashMap<PlayerId, super::MovementPolicy>>,
    resting_steps: HashMap<PlayerId, (PlayerStepInput, f32)>,
    resting_geometry: std::sync::Weak<CollisionIndex>,
    water_read: Option<Arc<dyn minimium_world::fluid::WaterRead>>,
    last_precise_batch_count: Option<usize>,
}

impl World {
    pub fn set_water_read(&mut self, water: Option<Arc<dyn minimium_world::fluid::WaterRead>>) {
        self.water_read = water;
    }

    fn water_sample(
        &self,
        position: [f32; 3],
        position_mm: Option<Position3Mm>,
    ) -> Option<minimium_world::fluid::WaterSample> {
        let water = self.water_read.as_ref()?;
        let feet = position_mm.map_or_else(
            || position.map(f64::from),
            |mm| mm.0.map(|v| v as f64 / 1000.0),
        );
        let up = match self.gravity {
            CharacterGravity::FixedY => [0.0, 1.0, 0.0],
            CharacterGravity::Radial { center } => {
                let delta = std::array::from_fn(|axis| feet[axis] - f64::from(center[axis]));
                let length = delta.iter().map(|v| v * v).sum::<f64>().sqrt();
                if length > f64::EPSILON {
                    delta.map(|v| v / length)
                } else {
                    [0.0, 1.0, 0.0]
                }
            }
        };
        Some(water.sample_capsule(feet, up, minimium_world::world::PLAYER_HEIGHT as f64))
    }

    pub fn movement_policy(&self) -> super::MovementPolicy {
        self.movement_policy
    }

    pub fn set_movement_policy(&mut self, policy: super::MovementPolicy) {
        if policy.valid() {
            self.movement_policy = policy;
        }
    }

    pub fn player_movement_policy(&self, player: PlayerId) -> super::MovementPolicy {
        self.player_movement_policies
            .get(&player)
            .copied()
            .unwrap_or(self.movement_policy)
    }

    pub fn set_player_movement_policy(&mut self, player: PlayerId, policy: super::MovementPolicy) {
        if policy.valid()
            && self.players.contains_key(&player)
            && self.player_movement_policy(player) != policy
        {
            Arc::make_mut(&mut self.player_movement_policies).insert(player, policy);
        }
    }
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
        let clear_of_boxes = position.iter().all(|value| value.is_finite())
            && match self.gravity {
                CharacterGravity::Radial { .. } => {
                    let up = character_frame(self.gravity, position).0;
                    self.spatial_geometry
                        .movement
                        .all_colliders()
                        .all(|collider| !oriented_capsule_intersects(position, up, *collider))
                }
                CharacterGravity::FixedY => self
                    .spatial_geometry
                    .movement
                    .all_colliders()
                    .all(|collider| !intersects_player(position, *collider)),
            };
        let position = if clear_of_boxes { position } else { self.spawn };
        let player = Player {
            id,
            name: Arc::from(name),
            position,
            position_mm: self.precise_positions.then(|| {
                if position == self.spawn {
                    self.spawn_mm.unwrap_or_else(|| {
                        Position3Mm(
                            position.map(|value| (f64::from(value) * 1000.0).round() as i64),
                        )
                    })
                } else {
                    Position3Mm(position.map(|value| (f64::from(value) * 1000.0).round() as i64))
                }
            }),
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
        self.resting_steps.remove(&id);
        Arc::make_mut(&mut self.players).remove(&id);
        Arc::make_mut(&mut self.player_movement_policies).remove(&id);
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
        inputs: &HashMap<PlayerId, MovementIntent>,
        delta_seconds: f32,
    ) -> CollisionQueryStats {
        self.step_players_selected(inputs, None, delta_seconds)
    }

    /// Advances only players whose required collision sectors are installed.
    /// Other players retain position and velocity until their geometry is ready.
    pub fn step_players_selected(
        &mut self,
        inputs: &HashMap<PlayerId, MovementIntent>,
        selected: Option<&HashSet<PlayerId>>,
        delta_seconds: f32,
    ) -> CollisionQueryStats {
        self.jump_resolutions.clear();
        let water_samples: HashMap<_, _> = self
            .players
            .iter()
            .filter(|(id, _)| selected.is_none_or(|ids| ids.contains(*id)))
            .filter_map(|(id, player)| {
                self.water_sample(player.position, player.position_mm)
                    .map(|sample| (*id, sample))
            })
            .collect();
        // A weak reference keeps the identity unique even after a publication is replaced.
        if !std::ptr::eq(
            self.resting_geometry.as_ptr(),
            Arc::as_ptr(&self.spatial_geometry.movement),
        ) {
            self.resting_steps.clear();
            self.resting_geometry = Arc::downgrade(&self.spatial_geometry.movement);
        }
        let Self {
            resting_steps,
            players,
            cells,
            player_cells,
            spatial_geometry,
            collision_query,
            gravity,
            movement_policy,
            player_movement_policies,
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
            let policy = player_movement_policies
                .get(id)
                .copied()
                .unwrap_or(*movement_policy);
            let water = water_samples.get(id).copied();
            let step = PlayerStepInput {
                id: *id,
                state,
                position_mm: player.position_mm,
                input,
                movement_policy: policy,
                water,
            };
            if !input.jump && resting_steps.get(id) == Some(&(step, delta_seconds)) {
                continue;
            }
            collision_stats.accumulate(collision_query.fill_for_policy(
                &spatial_geometry.movement,
                state,
                delta_seconds,
                policy,
            ));
            let jump = if let Some(position_mm) = &mut player.position_mm {
                precise_step::step_character_mm_with_water_outcome(
                    &mut state,
                    position_mm,
                    input,
                    policy,
                    collision_query.candidates(),
                    collision_query.surface_candidates(),
                    delta_seconds,
                    water,
                )
            } else {
                super::movement::step_character_with_water_outcome(
                    &mut state,
                    input,
                    policy,
                    collision_query.candidates(),
                    collision_query.surface_candidates(),
                    delta_seconds,
                    water,
                )
            };
            if let Some(jump) = jump {
                self.jump_resolutions.push((*id, jump));
            }
            if state == step.state && player.position_mm == step.position_mm {
                resting_steps.insert(*id, (step, delta_seconds));
            } else {
                resting_steps.remove(id);
            }
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
        inputs: &HashMap<PlayerId, MovementIntent>,
        delta_seconds: f32,
        lanes: usize,
        dispatch: impl FnOnce(Vec<SimulationBatch>, Arc<CollisionIndex>, f32) -> Vec<SimulationBatch>,
    ) -> CollisionQueryStats {
        self.step_players_with_selected(inputs, None, delta_seconds, lanes, dispatch)
    }

    /// Runs the selected players on the owner lane or in immutable worker batches.
    pub fn step_players_with_selected(
        &mut self,
        inputs: &HashMap<PlayerId, MovementIntent>,
        selected: Option<&HashSet<PlayerId>>,
        delta_seconds: f32,
        lanes: usize,
        dispatch: impl FnOnce(Vec<SimulationBatch>, Arc<CollisionIndex>, f32) -> Vec<SimulationBatch>,
    ) -> CollisionQueryStats {
        let Some((geometry, batches)) = self.prepare_movement_batches(inputs, selected, lanes)
        else {
            self.last_precise_batch_count = self.precise_positions.then_some(0);
            return self.step_players_selected(inputs, selected, delta_seconds);
        };
        self.last_precise_batch_count = self.precise_positions.then_some(batches.len());
        let results = dispatch(batches, geometry, delta_seconds);
        self.apply_step_results(results)
    }

    /// Returns whether precise movement used workers and how many batches were dispatched.
    pub fn precise_dispatch_stats(&self) -> Option<(bool, usize)> {
        self.last_precise_batch_count
            .map(|batches| (batches > 0, batches))
    }

    pub fn set_position(&mut self, id: PlayerId, position: [f32; 3]) -> bool {
        if !position.iter().all(|value| value.is_finite()) {
            return false;
        }
        let Some(player) = Arc::make_mut(&mut self.players).get_mut(&id) else {
            return false;
        };
        player.position = position;
        player.position_mm = self
            .precise_positions
            .then(|| Position3Mm(position.map(|value| (f64::from(value) * 1000.0).round() as i64)));
        player.velocity = [0.0; 3];
        player.grounded = false;
        self.update_player_cell(id, position);
        true
    }

    pub fn restore_position_mm(&mut self, id: PlayerId, position: Position3Mm) -> bool {
        if !self.precise_positions
            || position
                .0
                .iter()
                .any(|value| value.unsigned_abs() > 7_000_000_000)
        {
            return false;
        }
        let Some(player) = Arc::make_mut(&mut self.players).get_mut(&id) else {
            return false;
        };
        let approximate = position.0.map(|value| value as f32 / 1000.0);
        if player
            .position
            .iter()
            .zip(approximate)
            .any(|(a, b)| (a - b).abs() > 1.0)
        {
            return false;
        }
        player.position_mm = Some(position);
        player.position = approximate;
        self.update_player_cell(id, approximate);
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

    pub fn character_geometry(&self, entity: PlayerId) -> Option<CharacterGeometry> {
        let player = self.players.get(&entity)?;
        let feet_m = player.position_mm.map_or_else(
            || player.position.map(f64::from),
            |position| position.0.map(|value| value as f64 / 1000.0),
        );
        CharacterGeometry::new(feet_m, self.gravity)
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
