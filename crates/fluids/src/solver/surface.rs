use super::*;

impl FluidSnapshot {
    /// Shared active-cell observation used by server and client capsule readers.
    pub fn sample_cell(&self, key: [i32; 3]) -> Option<minimium_world::fluid::WaterSample> {
        let cell = self.cells().get(&key)?;
        if cell.capacity_m3 <= 0.0 {
            return None;
        }
        let mut velocity = [0.0; 3];
        for axis in 0..3 {
            let mut previous = key;
            previous[axis] = previous[axis].checked_sub(1)?;
            let negative = self
                .cells()
                .get(&previous)
                .map_or(cell.negative_boundary_velocity_m_s[axis], |neighbor| {
                    neighbor.positive_velocity_m_s[axis]
                });
            velocity[axis] = (cell.positive_velocity_m_s[axis] + negative) * 0.5;
        }
        Some(minimium_world::fluid::WaterSample {
            submerged_fraction: (cell.volume_m3 / cell.capacity_m3).clamp(0.0, 1.0),
            velocity_m_s: velocity,
        })
    }

    /// Bounded Youngs/PLIC reconstruction values for local surface rendering.
    pub fn interface_planes(&self) -> Vec<FluidInterfacePlane> {
        self.cells()
            .keys()
            .flat_map(|cell| self.interface_planes_for_cell(*cell))
            .collect()
    }

    /// Cheap local list of partial cells and full cells with a dry active neighbor.
    pub fn interface_cells_near(
        &self,
        center_m: [f64; 3],
        radius_m: f64,
        limit: usize,
    ) -> Vec<[i32; 3]> {
        if center_m.iter().any(|value| !value.is_finite())
            || !radius_m.is_finite()
            || radius_m <= 0.0
            || limit == 0
        {
            return Vec::new();
        }
        let radius_squared = radius_m * radius_m;
        let mut candidates: Vec<_> = self
            .cells()
            .iter()
            .filter_map(|(key, cell)| {
                self.has_interface(*key, cell)
                    .then(|| {
                        let point =
                            key.map(|value| f64::from(value) * CELL_SIZE_M + CELL_SIZE_M * 0.5);
                        let distance = point
                            .iter()
                            .zip(center_m)
                            .map(|(a, b)| (a - b).powi(2))
                            .sum::<f64>();
                        (distance, *key)
                    })
                    .filter(|(distance, _)| *distance <= radius_squared)
            })
            .collect();
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut selected: Vec<_> = candidates
            .into_iter()
            .take(limit)
            .map(|(_, key)| key)
            .collect();
        selected.sort_unstable();
        selected
    }

    pub fn interface_cells_limited(&self, limit: usize) -> Vec<[i32; 3]> {
        self.cells()
            .iter()
            .filter_map(|(key, cell)| self.has_interface(*key, cell).then_some(*key))
            .take(limit)
            .collect()
    }

    pub fn interface_planes_limited(&self, limit: usize) -> Vec<FluidInterfacePlane> {
        self.interface_cells_limited(limit)
            .into_iter()
            .flat_map(|cell| self.interface_planes_for_cell(cell))
            .collect()
    }

    pub fn interface_planes_near(
        &self,
        center_m: [f64; 3],
        radius_m: f64,
        limit: usize,
    ) -> Vec<FluidInterfacePlane> {
        self.interface_cells_near(center_m, radius_m, limit)
            .into_iter()
            .flat_map(|cell| self.interface_planes_for_cell(cell))
            .collect()
    }

    fn has_interface(&self, key: [i32; 3], cell: &crate::CellWater) -> bool {
        if cell.volume_m3 > 1.0e-12 && cell.volume_m3 < cell.capacity_m3 - 1.0e-12 {
            return true;
        }
        cell.capacity_m3 > 0.0
            && cell.volume_m3 > 1.0e-12
            && cell.volume_m3 >= cell.capacity_m3 - 1.0e-12
            && (0..3).any(|axis| {
                [-1, 1].into_iter().any(|sign| {
                    key[axis]
                        .checked_add(sign)
                        .and_then(|coordinate| {
                            let mut neighbor = key;
                            neighbor[axis] = coordinate;
                            self.cells().get(&neighbor)
                        })
                        .is_some_and(|neighbor| {
                            neighbor.capacity_m3 > 0.0 && neighbor.volume_m3 <= 1.0e-12
                        })
                })
            })
    }

    fn interface_planes_for_cell(&self, key: [i32; 3]) -> Vec<FluidInterfacePlane> {
        let Some(cell) = self.cells().get(&key) else {
            return Vec::new();
        };
        if cell.volume_m3 > 1.0e-12 && cell.volume_m3 < cell.capacity_m3 - 1.0e-12 {
            return crate::plic::reconstruct(self, key)
                .map(|plane| {
                    vec![FluidInterfacePlane {
                        cell: key,
                        normal: plane.normal,
                        offset_m: plane.offset,
                    }]
                })
                .unwrap_or_default();
        }
        if cell.capacity_m3 <= 0.0
            || cell.volume_m3 <= 1.0e-12
            || cell.volume_m3 < cell.capacity_m3 - 1.0e-12
        {
            return Vec::new();
        }
        let mut planes = Vec::new();
        for axis in 0..3 {
            for sign in [-1, 1] {
                let Some(coordinate) = key[axis].checked_add(sign) else {
                    continue;
                };
                let mut neighbor_key = key;
                neighbor_key[axis] = coordinate;
                let dry_active = self.cells().get(&neighbor_key).is_some_and(|neighbor| {
                    neighbor.capacity_m3 > 0.0 && neighbor.volume_m3 <= 1.0e-12
                });
                if !dry_active {
                    continue;
                }
                let mut normal = [0.0; 3];
                normal[axis] = f64::from(sign);
                planes.push(FluidInterfacePlane {
                    cell: key,
                    normal,
                    offset_m: CELL_SIZE_M * 0.5,
                });
            }
        }
        planes
    }
}
