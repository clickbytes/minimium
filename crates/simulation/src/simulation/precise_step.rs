use super::collision_queries::CollisionQueryStats;
use super::spatial::update_cell_index;
use super::world_state::{SimulationBatch, World};
use super::{CharacterGravity, CharacterState, MovementIntent};
use minimium_world::world::{Aabb, MapTriangle, Position3Mm};
use std::sync::Arc;

pub fn step_character_mm(
    state: &mut CharacterState,
    position_mm: &mut Position3Mm,
    input: MovementIntent,
    policy: super::MovementPolicy,
    colliders: &[Aabb],
    surfaces: &[MapTriangle],
    delta_seconds: f32,
) {
    step_character_mm_with_water(
        state,
        position_mm,
        input,
        policy,
        colliders,
        surfaces,
        delta_seconds,
        None,
    );
}

pub fn step_character_mm_with_water(
    state: &mut CharacterState,
    position_mm: &mut Position3Mm,
    input: MovementIntent,
    policy: super::MovementPolicy,
    colliders: &[Aabb],
    surfaces: &[MapTriangle],
    delta_seconds: f32,
    water: Option<minimium_world::fluid::WaterSample>,
) {
    let _ = step_character_mm_with_water_outcome(
        state,
        position_mm,
        input,
        policy,
        colliders,
        surfaces,
        delta_seconds,
        water,
    );
}

pub(super) fn step_character_mm_with_water_outcome(
    state: &mut CharacterState,
    position_mm: &mut Position3Mm,
    input: MovementIntent,
    policy: super::MovementPolicy,
    colliders: &[Aabb],
    surfaces: &[MapTriangle],
    delta_seconds: f32,
    water: Option<minimium_world::fluid::WaterSample>,
) -> Option<super::JumpResolution> {
    let origin = position_mm.0;
    let local = |point: [f32; 3]| -> [f32; 3] {
        std::array::from_fn(|axis| (f64::from(point[axis]) - origin[axis] as f64 / 1000.0) as f32)
    };
    let local_colliders: Vec<_> = colliders
        .iter()
        .map(|collider| Aabb {
            min: local(collider.min),
            max: local(collider.max),
        })
        .collect();
    let local_surfaces: Vec<_> = surfaces
        .iter()
        .map(|surface| MapTriangle {
            vertices: surface.precise_vertices.map_or_else(
                || surface.vertices.map(local),
                |precise| {
                    precise.map(|point| {
                        std::array::from_fn(|axis| (point[axis] - origin[axis]) as f32 / 1000.0)
                    })
                },
            ),
            precise_vertices: None,
            color: surface.color,
            solid: surface.solid,
            surface: surface.surface,
        })
        .collect();
    let original_gravity = state.gravity;
    let gravity = match original_gravity {
        CharacterGravity::Radial { center } => CharacterGravity::Radial {
            center: local(center),
        },
        other => other,
    };
    state.position = [0.0; 3];
    state.gravity = gravity;
    let jump = super::movement::step_character_with_water_outcome(
        state,
        input,
        policy,
        &local_colliders,
        &local_surfaces,
        delta_seconds,
        water,
    );
    position_mm.0 = std::array::from_fn(|axis| {
        origin[axis].saturating_add((f64::from(state.position[axis]) * 1000.0).round() as i64)
    });
    state.position = position_mm.0.map(|value| (value as f64 / 1000.0) as f32);
    state.gravity = original_gravity;
    jump
}

impl World {
    pub fn jump_resolutions(&self) -> &[(minimium_core::ids::PlayerId, super::JumpResolution)] {
        &self.jump_resolutions
    }
    pub(super) fn apply_step_results(
        &mut self,
        processed_batches: Vec<SimulationBatch>,
    ) -> CollisionQueryStats {
        self.jump_resolutions.clear();
        let players = Arc::make_mut(&mut self.players);
        let cells = Arc::make_mut(&mut self.cells);
        let player_cells = Arc::make_mut(&mut self.player_cells);
        let mut collision_stats = CollisionQueryStats::default();
        let mut results = Vec::new();
        for batch in &processed_batches {
            collision_stats.accumulate(batch.collision_stats);
            results.extend(batch.results.iter().copied());
        }
        results.sort_unstable_by_key(|result| result.id);
        for result in results {
            if let Some(player) = players.get_mut(&result.id) {
                player.position = result.state.position;
                player.position_mm = result.position_mm;
                player.velocity = result.state.velocity;
                player.grounded = result.state.grounded;
                update_cell_index(cells, player_cells, result.id, player.position);
                if let Some(jump) = result.jump {
                    self.jump_resolutions.push((result.id, jump));
                }
            }
        }
        self.simulation_batches = processed_batches;
        collision_stats
    }
}
