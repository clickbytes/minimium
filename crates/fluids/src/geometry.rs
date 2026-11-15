use crate::{FluidFace, FluidGeometry, WaterEndpoint, CELL_SIZE_M};
use minimium_world::terrain::TerrainRead;
use std::collections::BTreeMap;

/// Signed-distance terrain adapter for the shared 0.5 m water lattice.
/// Sample points integrate free volume/aperture; unknown terrain remains unknown.
pub struct TerrainFluidGeometry<'a> {
    terrain: &'a dyn TerrainRead,
    boundaries: BTreeMap<([i32; 3], usize, bool), WaterEndpoint>,
}

impl<'a> TerrainFluidGeometry<'a> {
    pub fn new(terrain: &'a dyn TerrainRead) -> Self {
        Self {
            terrain,
            boundaries: BTreeMap::new(),
        }
    }

    pub fn with_boundary(
        mut self,
        cell: [i32; 3],
        axis: usize,
        positive: bool,
        endpoint: WaterEndpoint,
    ) -> Option<Self> {
        if axis >= 3 || matches!(endpoint, WaterEndpoint::Solid | WaterEndpoint::Cell(_)) {
            return None;
        }
        self.boundaries.insert((cell, axis, positive), endpoint);
        Some(self)
    }

    fn free_fraction(&self, position: [f64; 3]) -> Option<f64> {
        let mut free = 0_u32;
        for x in 0..4 {
            for y in 0..4 {
                for z in 0..4 {
                    let offsets = [x, y, z].map(|v| (v as f64 + 0.5) * CELL_SIZE_M / 4.0);
                    let point = std::array::from_fn(|axis| position[axis] + offsets[axis]);
                    if self.terrain.sample(point)?.distance_m >= 0.0 {
                        free += 1;
                    }
                }
            }
        }
        Some(f64::from(free) / 64.0)
    }

    fn aperture(&self, cell: [i32; 3], axis: usize, positive: bool) -> Option<f64> {
        let origin = cell.map(|v| f64::from(v) * CELL_SIZE_M);
        let face = origin[axis] + if positive { CELL_SIZE_M } else { 0.0 };
        let mut free = 0_u32;
        for a in 0..8 {
            for b in 0..8 {
                let mut point = origin;
                point[axis] = face;
                point[(axis + 1) % 3] += (a as f64 + 0.5) * CELL_SIZE_M / 8.0;
                point[(axis + 2) % 3] += (b as f64 + 0.5) * CELL_SIZE_M / 8.0;
                if self.terrain.sample(point)?.distance_m >= 0.0 {
                    free += 1;
                }
            }
        }
        Some(f64::from(free) / 64.0 * CELL_SIZE_M * CELL_SIZE_M)
    }
}

impl FluidGeometry for TerrainFluidGeometry<'_> {
    fn capacity_m3(&self, cell: [i32; 3]) -> Option<f64> {
        let origin = cell.map(|v| f64::from(v) * CELL_SIZE_M);
        Some(self.free_fraction(origin)? * CELL_SIZE_M.powi(3))
    }

    fn face(&self, cell: [i32; 3], axis: usize, positive: bool) -> Option<FluidFace> {
        if axis >= 3 {
            return None;
        }
        let open_area_m2 = self.aperture(cell, axis, positive)?;
        let boundary = if open_area_m2 == 0.0 {
            WaterEndpoint::Solid
        } else if let Some(endpoint) = self.boundaries.get(&(cell, axis, positive)) {
            *endpoint
        } else {
            let mut neighbor = cell;
            neighbor[axis] = neighbor[axis].checked_add(if positive { 1 } else { -1 })?;
            if self.capacity_m3(neighbor)? <= 0.0 {
                WaterEndpoint::Solid
            } else {
                WaterEndpoint::Cell(neighbor)
            }
        };
        Some(FluidFace {
            open_area_m2,
            boundary,
        })
    }
}
