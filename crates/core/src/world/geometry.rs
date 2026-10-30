use super::collision_index::{Aabb, Cell};
use super::map::MapTriangle;
use super::{CELL_SIZE, COLLISION_EPSILON, PLAYER_HEIGHT, PLAYER_RADIUS};
use crate::ids::PlayerId;
use std::collections::HashMap;

pub fn triangle_area_squared(vertices: [[f32; 3]; 3]) -> f32 {
    let cross = triangle_normal(vertices);
    cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]
}

pub(super) fn triangle_normal(vertices: [[f32; 3]; 3]) -> [f32; 3] {
    let a = vertices[0];
    let b = vertices[1];
    let c = vertices[2];
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ]
}

pub(super) fn is_valid_solid_triangle(surface: MapTriangle) -> bool {
    if !surface.solid
        || surface
            .vertices
            .iter()
            .flatten()
            .any(|value| !value.is_finite())
    {
        return false;
    }
    if let Some(vertices) = surface.precise_vertices {
        let ab: [f64; 3] =
            std::array::from_fn(|axis| (vertices[1][axis] - vertices[0][axis]) as f64 / 1000.0);
        let ac: [f64; 3] =
            std::array::from_fn(|axis| (vertices[2][axis] - vertices[0][axis]) as f64 / 1000.0);
        let cross = [
            ab[1] * ac[2] - ab[2] * ac[1],
            ab[2] * ac[0] - ab[0] * ac[2],
            ab[0] * ac[1] - ab[1] * ac[0],
        ];
        return cross.iter().map(|value| value * value).sum::<f64>() > 1.0e-12;
    }
    let normal = triangle_normal(surface.vertices);
    if normal.iter().any(|value| !value.is_finite()) {
        return false;
    }
    let normal_length_squared =
        normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2];
    normal_length_squared.is_finite() && normal_length_squared > 1.0e-12
}

pub(super) fn is_walkable_triangle(surface: MapTriangle) -> bool {
    if !is_valid_solid_triangle(surface) {
        return false;
    }
    let normal = triangle_normal(surface.vertices);
    let normal_length_squared =
        normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2];
    normal[1].abs() >= normal_length_squared.sqrt() * 0.5
}

pub(super) fn triangle_height_for_player(position: [f32; 3], surface: MapTriangle) -> Option<f32> {
    if !is_walkable_triangle(surface) {
        return None;
    }
    let [a, b, c] = surface.vertices;
    let v0 = [c[0] - a[0], c[2] - a[2]];
    let v1 = [b[0] - a[0], b[2] - a[2]];
    let dot00 = v0[0] * v0[0] + v0[1] * v0[1];
    let dot01 = v0[0] * v1[0] + v0[1] * v1[1];
    let dot11 = v1[0] * v1[0] + v1[1] * v1[1];
    let denominator = dot00 * dot11 - dot01 * dot01;
    if !denominator.is_finite() || denominator.abs() <= 1.0e-12 {
        return None;
    }
    let inverse_denominator = denominator.recip();
    let normal = triangle_normal(surface.vertices);
    if normal[1].abs() <= 1.0e-6 {
        return None;
    }
    let sample_points = [
        [position[0], position[2]],
        [position[0] - PLAYER_RADIUS, position[2]],
        [position[0] + PLAYER_RADIUS, position[2]],
        [position[0], position[2] - PLAYER_RADIUS],
        [position[0], position[2] + PLAYER_RADIUS],
    ];
    let mut highest = None;
    for point in sample_points {
        let v2 = [point[0] - a[0], point[1] - a[2]];
        let dot02 = v0[0] * v2[0] + v0[1] * v2[1];
        let dot12 = v1[0] * v2[0] + v1[1] * v2[1];
        let u = (dot11 * dot02 - dot01 * dot12) * inverse_denominator;
        let v = (dot00 * dot12 - dot01 * dot02) * inverse_denominator;
        if u < -1.0e-5 || v < -1.0e-5 || u + v > 1.00001 {
            continue;
        }
        let height =
            a[1] - (normal[0] * (point[0] - a[0]) + normal[2] * (point[1] - a[2])) / normal[1];
        if height.is_finite() && highest.is_none_or(|current| height > current) {
            highest = Some(height);
        }
    }
    highest
}

pub(super) fn highest_triangle_height_for_player(
    position: [f32; 3],
    surfaces: &[MapTriangle],
) -> Option<f32> {
    surfaces
        .iter()
        .copied()
        .filter_map(|surface| triangle_height_for_player(position, surface))
        .max_by(f32::total_cmp)
}

pub(super) fn intersects_triangle_surface(position: [f32; 3], surface: MapTriangle) -> bool {
    triangle_height_for_player(position, surface).is_some_and(|height| {
        position[1] < height - COLLISION_EPSILON
            && position[1] + PLAYER_HEIGHT > height + COLLISION_EPSILON
    })
}

pub(super) fn triangle_intersects_player(position: [f32; 3], surface: MapTriangle) -> bool {
    if is_walkable_triangle(surface) {
        return intersects_triangle_surface(position, surface);
    }
    if !is_valid_solid_triangle(surface) {
        return false;
    }

    let center = [position[0], position[1] + PLAYER_HEIGHT * 0.5, position[2]];
    let half = [PLAYER_RADIUS, PLAYER_HEIGHT * 0.5, PLAYER_RADIUS];
    let vertices = surface.vertices.map(|vertex| {
        [
            vertex[0] - center[0],
            vertex[1] - center[1],
            vertex[2] - center[2],
        ]
    });

    for axis in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] {
        if triangle_separated_on_axis(vertices, half, axis) {
            return false;
        }
    }

    let edges = [
        vector_subtract(vertices[1], vertices[0]),
        vector_subtract(vertices[2], vertices[1]),
        vector_subtract(vertices[0], vertices[2]),
    ];
    let normal = triangle_normal(surface.vertices);
    if triangle_separated_on_axis(vertices, half, normal) {
        return false;
    }
    for edge in edges {
        for axis in [
            [0.0, edge[2], -edge[1]],
            [-edge[2], 0.0, edge[0]],
            [edge[1], -edge[0], 0.0],
        ] {
            if triangle_separated_on_axis(vertices, half, axis) {
                return false;
            }
        }
    }
    true
}

pub(super) fn clip_vertical_motion_to_triangles(
    position: [f32; 3],
    old_y: f32,
    new_y: f32,
    surfaces: &[MapTriangle],
) -> Option<f32> {
    let change = new_y - old_y;
    if !change.is_finite() || change == 0.0 {
        return None;
    }
    let steps = (change.abs() / PLAYER_RADIUS).ceil().clamp(1.0, 16.0) as u32;
    let mut previous_y = old_y;
    for step in 1..=steps {
        let fraction = step as f32 / steps as f32;
        let candidate_y = old_y + change * fraction;
        let mut candidate_position = position;
        candidate_position[1] = candidate_y;
        if surfaces.iter().copied().any(|surface| {
            !is_walkable_triangle(surface)
                && triangle_intersects_player(candidate_position, surface)
        }) {
            return Some(previous_y);
        }
        previous_y = candidate_y;
    }
    None
}

pub(super) fn triangle_separated_on_axis(
    vertices: [[f32; 3]; 3],
    half: [f32; 3],
    axis: [f32; 3],
) -> bool {
    let axis_length_squared = axis[0] * axis[0] + axis[1] * axis[1] + axis[2] * axis[2];
    if !axis_length_squared.is_finite() || axis_length_squared <= 1.0e-12 {
        return false;
    }
    let projection =
        |vertex: [f32; 3]| vertex[0] * axis[0] + vertex[1] * axis[1] + vertex[2] * axis[2];
    let first = projection(vertices[0]);
    let second = projection(vertices[1]);
    let third = projection(vertices[2]);
    let minimum = first.min(second).min(third);
    let maximum = first.max(second).max(third);
    let radius = half[0] * axis[0].abs() + half[1] * axis[1].abs() + half[2] * axis[2].abs();
    let margin = COLLISION_EPSILON * axis_length_squared.sqrt();
    minimum >= radius - margin || maximum <= -radius + margin
}

pub(super) fn vector_subtract(left: [f32; 3], right: [f32; 3]) -> [f32; 3] {
    [left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

pub(super) fn update_cell_index(
    cells: &mut HashMap<Cell, Vec<PlayerId>>,
    player_cells: &mut HashMap<PlayerId, Cell>,
    id: PlayerId,
    position: [f32; 3],
) {
    let new_cell = cell_for(position);
    if player_cells.get(&id).copied() == Some(new_cell) {
        return;
    }
    let old_cell = player_cells.insert(id, new_cell);
    if old_cell != Some(new_cell) {
        if let Some(old_cell) = old_cell {
            let empty = if let Some(players) = cells.get_mut(&old_cell) {
                players.retain(|player| *player != id);
                players.is_empty()
            } else {
                false
            };
            if empty {
                cells.remove(&old_cell);
            }
        }
        cells.entry(new_cell).or_default().push(id);
    }
}

pub(super) fn horizontal_overlap(position: [f32; 3], collider: Aabb) -> bool {
    let closest_x = position[0].clamp(collider.min[0], collider.max[0]);
    let closest_z = position[2].clamp(collider.min[2], collider.max[2]);
    let delta_x = position[0] - closest_x;
    let delta_z = position[2] - closest_z;
    delta_x * delta_x + delta_z * delta_z <= PLAYER_RADIUS * PLAYER_RADIUS
}

pub(super) fn intersects_player(position: [f32; 3], collider: Aabb) -> bool {
    oriented_capsule_intersects(position, [0.0, 1.0, 0.0], collider)
}

pub(super) fn oriented_capsule_intersects(
    position: [f32; 3],
    up: [f32; 3],
    collider: Aabb,
) -> bool {
    let half_segment = (PLAYER_HEIGHT * 0.5 - PLAYER_RADIUS).max(0.0);
    let center = [
        position[0] + up[0] * PLAYER_HEIGHT * 0.5,
        position[1] + up[1] * PLAYER_HEIGHT * 0.5,
        position[2] + up[2] * PLAYER_HEIGHT * 0.5,
    ];
    let offset = [
        up[0] * half_segment,
        up[1] * half_segment,
        up[2] * half_segment,
    ];
    let segment_start = vector_subtract(center, offset);
    let segment_end = [
        center[0] + offset[0],
        center[1] + offset[1],
        center[2] + offset[2],
    ];
    segment_aabb_distance_squared(segment_start, segment_end, collider)
        < PLAYER_RADIUS * PLAYER_RADIUS
}

pub(super) fn oriented_capsule_intersects_triangle(
    position: [f32; 3],
    up: [f32; 3],
    triangle: MapTriangle,
) -> bool {
    if !is_valid_solid_triangle(triangle) {
        return false;
    }
    let half_segment = (PLAYER_HEIGHT * 0.5 - PLAYER_RADIUS).max(0.0);
    let center = [
        position[0] + up[0] * PLAYER_HEIGHT * 0.5,
        position[1] + up[1] * PLAYER_HEIGHT * 0.5,
        position[2] + up[2] * PLAYER_HEIGHT * 0.5,
    ];
    let offset = up.map(|component| component * half_segment);
    let start = vector_subtract(center, offset);
    let end = [
        center[0] + offset[0],
        center[1] + offset[1],
        center[2] + offset[2],
    ];
    triangle_distance::segment_triangle_distance_squared(start, end, triangle.vertices)
        < PLAYER_RADIUS * PLAYER_RADIUS
}

fn dot3(first: [f32; 3], second: [f32; 3]) -> f32 {
    first[0] * second[0] + first[1] * second[1] + first[2] * second[2]
}

fn segment_aabb_distance_squared(start: [f32; 3], end: [f32; 3], collider: Aabb) -> f32 {
    let direction = vector_subtract(end, start);
    let mut breakpoints = [0.0_f32; 8];
    let mut breakpoint_count = 2;
    breakpoints[1] = 1.0;
    for axis in 0..3 {
        if direction[axis] == 0.0 {
            continue;
        }
        for boundary in [collider.min[axis], collider.max[axis]] {
            let parameter = (boundary - start[axis]) / direction[axis];
            if parameter > 0.0 && parameter < 1.0 {
                breakpoints[breakpoint_count] = parameter;
                breakpoint_count += 1;
            }
        }
    }
    breakpoints[..breakpoint_count].sort_by(f32::total_cmp);

    let mut minimum = f32::INFINITY;
    for interval in breakpoints[..breakpoint_count].windows(2) {
        let lower = interval[0];
        let upper = interval[1];
        let middle = (lower + upper) * 0.5;
        let sample = [
            start[0] + direction[0] * middle,
            start[1] + direction[1] * middle,
            start[2] + direction[2] * middle,
        ];
        let mut numerator = 0.0;
        let mut denominator = 0.0;
        for axis in 0..3 {
            let boundary = if sample[axis] < collider.min[axis] {
                Some(collider.min[axis])
            } else if sample[axis] > collider.max[axis] {
                Some(collider.max[axis])
            } else {
                None
            };
            if let Some(boundary) = boundary {
                numerator += direction[axis] * (start[axis] - boundary);
                denominator += direction[axis] * direction[axis];
            }
        }
        let parameter = if denominator > 0.0 {
            (-numerator / denominator).clamp(lower, upper)
        } else {
            lower
        };
        let point = [
            start[0] + direction[0] * parameter,
            start[1] + direction[1] * parameter,
            start[2] + direction[2] * parameter,
        ];
        let mut distance_squared = 0.0;
        for (axis, coordinate) in point.iter().enumerate() {
            let closest = coordinate.clamp(collider.min[axis], collider.max[axis]);
            let difference = *coordinate - closest;
            distance_squared += difference * difference;
        }
        minimum = minimum.min(distance_squared);
    }
    minimum
}

mod triangle_distance;

#[cfg(test)]
mod tests;

pub(super) fn cell_for(position: [f32; 3]) -> Cell {
    Cell(
        (position[0] / CELL_SIZE).floor() as i32,
        (position[1] / CELL_SIZE).floor() as i32,
        (position[2] / CELL_SIZE).floor() as i32,
    )
}
