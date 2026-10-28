use super::collision_queries::cell_volume;
use super::geometry::{cell_for, is_valid_solid_triangle};
use super::map::MapTriangle;
use super::MAX_COLLIDER_CELLS;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct Cell(pub(super) i32, pub(super) i32, pub(super) i32);

#[derive(Clone, Copy, Debug)]
pub struct Aabb {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Aabb {
    pub fn from_center_size(center: [f32; 3], size: [f32; 3]) -> Self {
        let half = [size[0] * 0.5, size[1] * 0.5, size[2] * 0.5];
        Self {
            min: [
                center[0] - half[0],
                center[1] - half[1],
                center[2] - half[2],
            ],
            max: [
                center[0] + half[0],
                center[1] + half[1],
                center[2] + half[2],
            ],
        }
    }
}

#[derive(Clone, Default)]
pub struct CollisionIndex {
    pub(super) colliders: Vec<Aabb>,
    pub(super) cells: HashMap<Cell, Vec<usize>>,
    pub(super) large_colliders: Vec<usize>,
    pub(super) surfaces: Vec<MapTriangle>,
    pub(super) surface_cells: HashMap<Cell, Vec<usize>>,
    pub(super) large_surfaces: Vec<usize>,
}

impl CollisionIndex {
    pub fn from_colliders(colliders: Vec<Aabb>) -> Self {
        Self::from_geometry(colliders, Vec::new())
    }

    pub fn from_geometry(colliders: Vec<Aabb>, surfaces: Vec<MapTriangle>) -> Self {
        let mut index = Self::default();
        for collider in colliders {
            index.add(collider);
        }
        for surface in surfaces {
            if surface.solid {
                index.add_surface(surface);
            }
        }
        index
    }

    pub(super) fn add(&mut self, collider: Aabb) {
        if collider
            .min
            .iter()
            .chain(collider.max.iter())
            .any(|value| !value.is_finite())
            || (0..3).any(|axis| collider.min[axis] > collider.max[axis])
        {
            return;
        }

        let index = self.colliders.len();
        self.colliders.push(collider);
        let min = cell_for(collider.min);
        let max = cell_for(collider.max);
        let cell_count = cell_volume(min, max);
        if cell_count > MAX_COLLIDER_CELLS {
            self.large_colliders.push(index);
            return;
        }

        for z in min.2..=max.2 {
            for y in min.1..=max.1 {
                for x in min.0..=max.0 {
                    self.cells.entry(Cell(x, y, z)).or_default().push(index);
                }
            }
        }
    }

    fn add_surface(&mut self, surface: MapTriangle) {
        if !is_valid_solid_triangle(surface) {
            return;
        }
        let mut min = surface.vertices[0];
        let mut max = surface.vertices[0];
        for vertex in surface.vertices.iter().skip(1) {
            for axis in 0..3 {
                min[axis] = min[axis].min(vertex[axis]);
                max[axis] = max[axis].max(vertex[axis]);
            }
        }
        let index = self.surfaces.len();
        self.surfaces.push(surface);
        let min_cell = cell_for(min);
        let max_cell = cell_for(max);
        if cell_volume(min_cell, max_cell) > MAX_COLLIDER_CELLS {
            self.large_surfaces.push(index);
            return;
        }
        for z in min_cell.2..=max_cell.2 {
            for y in min_cell.1..=max_cell.1 {
                for x in min_cell.0..=max_cell.0 {
                    self.surface_cells
                        .entry(Cell(x, y, z))
                        .or_default()
                        .push(index);
                }
            }
        }
    }
}
