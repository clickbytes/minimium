use super::collision_index::CollisionIndex;
use super::MAX_RAY_CELL_CROSSINGS;
use minimium_core::ids::PlayerId;
use minimium_world::world::{cell_for, Aabb, Cell, MapTriangle, CELL_SIZE};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(super) struct PlayerRayQuery {
    pub(super) candidates: Vec<PlayerId>,
    pub(super) seen: HashSet<PlayerId>,
}

impl PlayerRayQuery {
    pub(super) fn fill(
        &mut self,
        cells: &HashMap<Cell, Vec<PlayerId>>,
        origin: [f32; 3],
        direction: [f32; 3],
        maximum_distance: f32,
    ) {
        self.candidates.clear();
        self.seen.clear();
        let candidates = &mut self.candidates;
        let seen = &mut self.seen;
        visit_ray_cells(origin, direction, maximum_distance, |cell| {
            for z_offset in -1..=1 {
                for y_offset in -1..=1 {
                    for x_offset in -1..=1 {
                        let nearby = Cell(cell.0 + x_offset, cell.1 + y_offset, cell.2 + z_offset);
                        if let Some(ids) = cells.get(&nearby) {
                            for id in ids {
                                if seen.insert(*id) {
                                    candidates.push(*id);
                                }
                            }
                        }
                    }
                }
            }
        });
    }
}

#[derive(Default)]
pub(super) struct MapRayQuery {
    pub(super) partition_candidates: Vec<usize>,
    pub(super) partition_query: Option<Box<MapRayQuery>>,
    pub(super) partition_hit: Option<f32>,
    pub(super) collider_candidates: Vec<usize>,
    pub(super) seen_colliders: Vec<u32>,
    pub(super) surface_candidates: Vec<usize>,
    pub(super) seen_surfaces: Vec<u32>,
    pub(super) generation: u32,
}

#[derive(Default)]
pub struct SkyCoverQuery {
    query: MapRayQuery,
}

impl SkyCoverQuery {
    pub fn covered(
        &mut self,
        index: &CollisionIndex,
        eye: [f32; 3],
        up: [f32; 3],
        reach: f32,
    ) -> bool {
        if !eye.iter().chain(up.iter()).all(|value| value.is_finite()) || reach <= 0.0 {
            return false;
        }
        let origin = std::array::from_fn(|axis| eye[axis] + up[axis] * 0.2);
        self.query.fill(index, origin, up, reach);
        self.query
            .nearest_blocker(index, origin, up, reach)
            .is_some_and(|distance| distance > 0.05)
    }
}

impl MapRayQuery {
    pub(super) fn fill(
        &mut self,
        index: &CollisionIndex,
        origin: [f32; 3],
        direction: [f32; 3],
        maximum_distance: f32,
    ) {
        self.collider_candidates.clear();
        self.surface_candidates.clear();
        if !index.partitions.is_empty() {
            self.fill_partitions(index, origin, direction, maximum_distance);
            return;
        }
        let mut reset_generation = false;
        if self.seen_colliders.len() != index.colliders.len() {
            self.seen_colliders.resize(index.colliders.len(), 0);
            reset_generation = true;
        }
        if self.seen_surfaces.len() != index.surfaces.len() {
            self.seen_surfaces.resize(index.surfaces.len(), 0);
            reset_generation = true;
        }
        if reset_generation {
            self.seen_colliders.fill(0);
            self.seen_surfaces.fill(0);
            self.generation = 0;
        }
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.seen_colliders.fill(0);
            self.seen_surfaces.fill(0);
            self.generation = 1;
        }
        let generation = self.generation;
        for candidate in &index.large_colliders {
            self.mark_collider(*candidate, generation);
        }
        for candidate in &index.large_surfaces {
            self.mark_surface(*candidate, generation);
        }
        let (seen_colliders, collider_candidates) =
            (&mut self.seen_colliders, &mut self.collider_candidates);
        let (seen_surfaces, surface_candidates) =
            (&mut self.seen_surfaces, &mut self.surface_candidates);
        visit_ray_cells(origin, direction, maximum_distance, |cell| {
            if let Some(indices) = index.cells.get(&cell) {
                for candidate in indices {
                    let candidate = *candidate;
                    if seen_colliders[candidate] != generation {
                        seen_colliders[candidate] = generation;
                        collider_candidates.push(candidate);
                    }
                }
            }
            if let Some(indices) = index.surface_cells.get(&cell) {
                for candidate in indices {
                    let candidate = *candidate;
                    if seen_surfaces[candidate] != generation {
                        seen_surfaces[candidate] = generation;
                        surface_candidates.push(candidate);
                    }
                }
            }
        });
    }

    fn mark_collider(&mut self, candidate: usize, generation: u32) {
        if self.seen_colliders[candidate] != generation {
            self.seen_colliders[candidate] = generation;
            self.collider_candidates.push(candidate);
        }
    }

    fn mark_surface(&mut self, candidate: usize, generation: u32) {
        if self.seen_surfaces[candidate] != generation {
            self.seen_surfaces[candidate] = generation;
            self.surface_candidates.push(candidate);
        }
    }

    pub(super) fn nearest_blocker(
        &self,
        index: &CollisionIndex,
        origin: [f32; 3],
        direction: [f32; 3],
        maximum_distance: f32,
    ) -> Option<f32> {
        if !index.partitions.is_empty() {
            return self.partition_hit;
        }
        let collider_distance = self
            .collider_candidates
            .iter()
            .filter_map(|candidate| {
                ray_aabb_distance(
                    origin,
                    direction,
                    index.colliders[*candidate],
                    maximum_distance,
                )
            })
            .min_by(f32::total_cmp);
        let surface_distance = self
            .surface_candidates
            .iter()
            .filter_map(|candidate| {
                ray_triangle_distance(
                    origin,
                    direction,
                    index.surfaces[*candidate],
                    maximum_distance,
                )
            })
            .min_by(f32::total_cmp);
        match (collider_distance, surface_distance) {
            (Some(left), Some(right)) => Some(left.min(right)),
            (Some(distance), None) | (None, Some(distance)) => Some(distance),
            (None, None) => None,
        }
    }

    pub(super) fn nearest_blocker_intersection(
        &self,
        index: &CollisionIndex,
        origin: [f32; 3],
        direction: [f32; 3],
        maximum_distance: f32,
        mut accepted_distance: impl FnMut(f32, f32) -> Option<f32>,
    ) -> Option<f32> {
        if !index.partitions.is_empty() {
            return None;
        }
        let mut nearest: Option<f32> = None;
        for candidate in &self.collider_candidates {
            let Some((near, far)) = ray_aabb_interval(
                origin,
                direction,
                index.colliders[*candidate],
                maximum_distance,
            ) else {
                continue;
            };
            let hit = accepted_distance(near, far.min(maximum_distance));
            if let Some(hit) = hit {
                nearest = Some(nearest.map_or(hit, |old| old.min(hit)));
            }
        }
        for candidate in &self.surface_candidates {
            if let Some(distance) = ray_triangle_distance(
                origin,
                direction,
                index.surfaces[*candidate],
                maximum_distance,
            ) {
                if let Some(hit) = accepted_distance(distance, distance) {
                    nearest = Some(nearest.map_or(hit, |old| old.min(hit)));
                }
            }
        }
        nearest
    }
}

pub(super) fn visit_ray_cells(
    origin: [f32; 3],
    direction: [f32; 3],
    maximum_distance: f32,
    mut visit: impl FnMut(Cell),
) {
    if !maximum_distance.is_finite()
        || maximum_distance <= 0.0
        || origin
            .iter()
            .chain(direction.iter())
            .any(|value| !value.is_finite())
    {
        return;
    }
    let start = cell_for(origin);
    let mut coordinates = [start.0, start.1, start.2];
    let mut step = [0i32; 3];
    let mut next_crossing = [f32::INFINITY; 3];
    let mut crossing_delta = [f32::INFINITY; 3];
    for axis in 0..3 {
        if direction[axis] > 0.0 {
            step[axis] = 1;
            let boundary = (coordinates[axis] + 1) as f32 * CELL_SIZE;
            next_crossing[axis] = (boundary - origin[axis]) / direction[axis];
            crossing_delta[axis] = CELL_SIZE / direction[axis];
        } else if direction[axis] < 0.0 {
            step[axis] = -1;
            let boundary = coordinates[axis] as f32 * CELL_SIZE;
            next_crossing[axis] = (boundary - origin[axis]) / direction[axis];
            crossing_delta[axis] = CELL_SIZE / -direction[axis];
        }
    }
    visit(Cell(coordinates[0], coordinates[1], coordinates[2]));
    for _ in 0..MAX_RAY_CELL_CROSSINGS {
        let crossing = next_crossing.iter().copied().fold(f32::INFINITY, f32::min);
        if !crossing.is_finite() || crossing > maximum_distance {
            break;
        }
        let mut tied_axes = 0u8;
        for (axis, value) in next_crossing.iter().enumerate() {
            if (*value - crossing).abs() <= 1.0e-5 {
                tied_axes |= 1 << axis;
            }
        }
        for subset in 1u8..8 {
            if subset & !tied_axes != 0 {
                continue;
            }
            let mut adjacent = coordinates;
            for axis in 0..3 {
                if subset & (1 << axis) != 0 {
                    adjacent[axis] += step[axis];
                }
            }
            visit(Cell(adjacent[0], adjacent[1], adjacent[2]));
        }
        for axis in 0..3 {
            if tied_axes & (1 << axis) != 0 {
                coordinates[axis] += step[axis];
                next_crossing[axis] += crossing_delta[axis];
            }
        }
    }
}

pub(super) fn ray_aabb_distance(
    origin: [f32; 3],
    direction: [f32; 3],
    bounds: Aabb,
    maximum_distance: f32,
) -> Option<f32> {
    ray_aabb_interval(origin, direction, bounds, maximum_distance).map(|(near, _)| near)
}

fn ray_aabb_interval(
    origin: [f32; 3],
    direction: [f32; 3],
    bounds: Aabb,
    maximum_distance: f32,
) -> Option<(f32, f32)> {
    let mut near = f32::NEG_INFINITY;
    let mut far = f32::INFINITY;
    for axis in 0..3 {
        if direction[axis].abs() <= 1.0e-8 {
            if origin[axis] < bounds.min[axis] || origin[axis] > bounds.max[axis] {
                return None;
            }
            continue;
        }
        let inverse = direction[axis].recip();
        let mut first = (bounds.min[axis] - origin[axis]) * inverse;
        let mut second = (bounds.max[axis] - origin[axis]) * inverse;
        if first > second {
            std::mem::swap(&mut first, &mut second);
        }
        near = near.max(first);
        far = far.min(second);
        if near > far {
            return None;
        }
    }
    let near = near.max(0.0);
    (far >= near && near <= maximum_distance).then_some((near, far.max(0.0)))
}

pub(super) fn ray_triangle_distance(
    origin: [f32; 3],
    direction: [f32; 3],
    triangle: MapTriangle,
    maximum_distance: f32,
) -> Option<f32> {
    let edge_one = subtract(triangle.vertices[1], triangle.vertices[0]);
    let edge_two = subtract(triangle.vertices[2], triangle.vertices[0]);
    let direction_cross_edge = cross(direction, edge_two);
    let determinant = dot(edge_one, direction_cross_edge);
    if determinant.abs() <= 1.0e-7 {
        return None;
    }
    let inverse_determinant = determinant.recip();
    let from_vertex = subtract(origin, triangle.vertices[0]);
    let u = dot(from_vertex, direction_cross_edge) * inverse_determinant;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = cross(from_vertex, edge_one);
    let v = dot(direction, q) * inverse_determinant;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let distance = dot(edge_two, q) * inverse_determinant;
    (distance >= 0.0 && distance <= maximum_distance).then_some(distance)
}

pub(super) fn subtract(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

pub(super) fn dot(left: [f32; 3], right: [f32; 3]) -> f32 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

pub(super) fn cross(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [
        left[1] * right[2] - left[2] * right[1],
        left[2] * right[0] - left[0] * right[2],
        left[0] * right[1] - left[1] * right[0],
    ]
}
