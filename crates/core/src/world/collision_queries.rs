use super::collision_index::{Aabb, Cell, CollisionIndex};
use super::geometry::cell_for;
use super::map::MapTriangle;
use super::rules::CharacterState;
use super::{
    GRAVITY, MAX_FALL_SPEED, MAX_QUERY_CELLS, MAX_STEP_UP, PLAYER_HEIGHT, PLAYER_JUMP_SPEED,
    PLAYER_MOVE_SPEED, PLAYER_RADIUS,
};

#[derive(Default)]
pub struct CollisionQuery {
    pub(super) candidates: Vec<Aabb>,
    pub(super) seen: Vec<u32>,
    pub(super) surface_candidates: Vec<MapTriangle>,
    pub(super) seen_surfaces: Vec<u32>,
    pub(super) generation: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CollisionQueryStats {
    pub queries: u64,
    pub cell_visits: u64,
    pub aabb_candidates: u64,
    pub triangle_candidates: u64,
    pub fallback_queries: u64,
    pub max_cell_visits: u64,
    pub max_aabb_candidates: u64,
    pub max_triangle_candidates: u64,
}

impl CollisionQueryStats {
    pub(super) fn accumulate(&mut self, other: Self) {
        self.queries = self.queries.saturating_add(other.queries);
        self.cell_visits = self.cell_visits.saturating_add(other.cell_visits);
        self.aabb_candidates = self.aabb_candidates.saturating_add(other.aabb_candidates);
        self.triangle_candidates = self
            .triangle_candidates
            .saturating_add(other.triangle_candidates);
        self.fallback_queries = self.fallback_queries.saturating_add(other.fallback_queries);
        self.max_cell_visits = self.max_cell_visits.max(other.max_cell_visits);
        self.max_aabb_candidates = self.max_aabb_candidates.max(other.max_aabb_candidates);
        self.max_triangle_candidates = self
            .max_triangle_candidates
            .max(other.max_triangle_candidates);
    }
}

impl CollisionQuery {
    pub fn fill(
        &mut self,
        index: &CollisionIndex,
        state: CharacterState,
        delta_seconds: f32,
    ) -> CollisionQueryStats {
        let mut stats = CollisionQueryStats {
            queries: 1,
            ..CollisionQueryStats::default()
        };
        self.candidates.clear();
        self.surface_candidates.clear();
        let mut reset_generation = false;
        if self.seen.len() != index.colliders.len() {
            self.seen.resize(index.colliders.len(), 0);
            reset_generation = true;
        }
        if self.seen_surfaces.len() != index.surfaces.len() {
            self.seen_surfaces.resize(index.surfaces.len(), 0);
            reset_generation = true;
        }
        if reset_generation {
            self.seen.fill(0);
            self.seen_surfaces.fill(0);
            self.generation = 0;
        }
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.seen.fill(0);
            self.generation = 1;
        }

        let generation = self.generation;
        for collider in &index.large_colliders {
            self.mark_candidate(index, *collider, generation);
        }
        for surface in &index.large_surfaces {
            self.mark_surface_candidate(index, *surface, generation);
        }

        let horizontal_reach = PLAYER_MOVE_SPEED * delta_seconds + PLAYER_RADIUS;
        let fall_reach = MAX_FALL_SPEED * delta_seconds + GRAVITY * delta_seconds * delta_seconds;
        let rise_reach = PLAYER_JUMP_SPEED * delta_seconds
            + GRAVITY * delta_seconds * delta_seconds
            + MAX_STEP_UP;
        let min = [
            state.position[0] - horizontal_reach,
            state.position[1] - fall_reach,
            state.position[2] - horizontal_reach,
        ];
        let max = [
            state.position[0] + horizontal_reach,
            state.position[1] + PLAYER_HEIGHT + rise_reach,
            state.position[2] + horizontal_reach,
        ];

        if min.iter().chain(max.iter()).any(|value| !value.is_finite()) {
            stats.fallback_queries = 1;
            for candidate in 0..index.colliders.len() {
                self.mark_candidate(index, candidate, generation);
            }
            for candidate in 0..index.surfaces.len() {
                self.mark_surface_candidate(index, candidate, generation);
            }
            return self.finish_fill(stats);
        }

        let min_cell = cell_for(min);
        let max_cell = cell_for(max);
        if cell_volume(min_cell, max_cell) > MAX_QUERY_CELLS {
            stats.fallback_queries = 1;
            for candidate in 0..index.colliders.len() {
                self.mark_candidate(index, candidate, generation);
            }
            for candidate in 0..index.surfaces.len() {
                self.mark_surface_candidate(index, candidate, generation);
            }
            return self.finish_fill(stats);
        }

        for z in min_cell.2..=max_cell.2 {
            for y in min_cell.1..=max_cell.1 {
                for x in min_cell.0..=max_cell.0 {
                    stats.cell_visits = stats.cell_visits.saturating_add(1);
                    if let Some(candidates) = index.cells.get(&Cell(x, y, z)) {
                        for candidate in candidates {
                            self.mark_candidate(index, *candidate, generation);
                        }
                    }
                    if let Some(candidates) = index.surface_cells.get(&Cell(x, y, z)) {
                        for candidate in candidates {
                            self.mark_surface_candidate(index, *candidate, generation);
                        }
                    }
                }
            }
        }
        self.finish_fill(stats)
    }

    pub fn candidates(&self) -> &[Aabb] {
        &self.candidates
    }

    pub fn surface_candidates(&self) -> &[MapTriangle] {
        &self.surface_candidates
    }

    fn mark_candidate(&mut self, index: &CollisionIndex, candidate: usize, generation: u32) {
        if self.seen[candidate] != generation {
            self.seen[candidate] = generation;
            self.candidates.push(index.colliders[candidate]);
        }
    }

    fn mark_surface_candidate(
        &mut self,
        index: &CollisionIndex,
        candidate: usize,
        generation: u32,
    ) {
        if self.seen_surfaces[candidate] != generation {
            self.seen_surfaces[candidate] = generation;
            self.surface_candidates.push(index.surfaces[candidate]);
        }
    }

    fn finish_fill(&self, mut stats: CollisionQueryStats) -> CollisionQueryStats {
        stats.aabb_candidates = self.candidates.len() as u64;
        stats.triangle_candidates = self.surface_candidates.len() as u64;
        stats.max_cell_visits = stats.cell_visits;
        stats.max_aabb_candidates = stats.aabb_candidates;
        stats.max_triangle_candidates = stats.triangle_candidates;
        stats
    }
}

pub(super) fn cell_volume(min: Cell, max: Cell) -> u64 {
    let width = (i64::from(max.0) - i64::from(min.0) + 1).max(0) as u64;
    let height = (i64::from(max.1) - i64::from(min.1) + 1).max(0) as u64;
    let depth = (i64::from(max.2) - i64::from(min.2) + 1).max(0) as u64;
    width.saturating_mul(height).saturating_mul(depth)
}
