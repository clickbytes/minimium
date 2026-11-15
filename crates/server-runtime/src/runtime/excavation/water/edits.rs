use super::{PreparedWaterEdit, WaterWorld};
use minimium_fluids::{
    FluidError, FluidGeometry, FluidSnapshotExt, TerrainFluidGeometry, CELL_SIZE_M,
};
use minimium_world::{terrain::TerrainRead, terrain_edits::CraterStamp};

pub(super) fn prepare_crater_water(
    owner: &WaterWorld,
    old_terrain: &dyn TerrainRead,
    new_terrain: &dyn TerrainRead,
    crater: CraterStamp,
    server_cell_limit: usize,
) -> Result<PreparedWaterEdit, FluidError> {
    if !crater.valid() || !(1..=524_288).contains(&server_cell_limit) {
        return Err(FluidError::Invalid);
    }
    let radius = crater.radius_m * 2.0 + CELL_SIZE_M;
    let low = crater
        .center
        .map(|value| ((value - radius) / CELL_SIZE_M).floor() as i32);
    let high = crater
        .center
        .map(|value| ((value + radius) / CELL_SIZE_M).floor() as i32);
    let scan_count: u64 = (0..3)
        .map(|axis| (i64::from(high[axis]) - i64::from(low[axis]) + 1).max(0) as u64)
        .product();
    let nearest_radius = (crater
        .center
        .iter()
        .map(|value| value * value)
        .sum::<f64>()
        .sqrt()
        - radius)
        .max(0.0);
    let nearest_bodies = owner.catalog.as_ref().map_or_else(Vec::new, |catalog| {
        let center_distance = crater
            .center
            .iter()
            .map(|value| value * value)
            .sum::<f64>()
            .sqrt();
        let direction = if center_distance > f64::EPSILON {
            crater.center.map(|value| value / center_distance)
        } else {
            [0.0, 1.0, 0.0]
        };
        let angular_radius = if nearest_radius <= 0.0 {
            std::f64::consts::PI
        } else {
            (radius / center_distance).clamp(0.0, 1.0).asin()
        };
        catalog.body_ids_near_direction(direction, angular_radius)
    });
    let basin_scan_needed = nearest_bodies.iter().any(|id| {
        owner
            .snapshot
            .reservoirs()
            .get(id)
            .and_then(|reservoir| reservoir.level_m().ok())
            .is_some_and(|level| nearest_radius <= level)
    });
    if basin_scan_needed && scan_count > 65_536 {
        return Err(FluidError::Capacity);
    }

    let mut candidates = std::collections::BTreeSet::new();
    for cell in owner.snapshot.cells().keys().copied() {
        if cell_distance(cell, crater.center) > radius {
            continue;
        }
        candidates.insert(cell);
        for axis in 0..3 {
            for sign in [-1, 1] {
                let mut neighbor = cell;
                if let Some(value) = neighbor[axis].checked_add(sign) {
                    neighbor[axis] = value;
                    candidates.insert(neighbor);
                }
            }
        }
    }
    if candidates.len() > 65_536 {
        return Err(FluidError::Capacity);
    }

    let old_geometry = TerrainFluidGeometry::new(old_terrain);
    let new_geometry = TerrainFluidGeometry::new(new_terrain);
    let mut staged = owner.snapshot.clone();
    if basin_scan_needed {
        for x in low[0]..=high[0] {
            for y in low[1]..=high[1] {
                for z in low[2]..=high[2] {
                    let cell = [x, y, z];
                    if cell_distance(cell, crater.center) > radius {
                        continue;
                    }
                    let old_capacity = old_geometry
                        .capacity_m3(cell)
                        .ok_or(FluidError::Unavailable)?;
                    if old_capacity <= 0.0 || staged.cells().contains_key(&cell) {
                        continue;
                    }
                    if owner.external_water_body(&staged, cell, [0.0; 3]).is_some() {
                        owner.activate_with_external_water(&mut staged, cell, old_capacity)?;
                        candidates.insert(cell);
                        check_budget(owner, &staged, server_cell_limit)?;
                    }
                }
            }
        }
    }

    let mut changes = Vec::new();
    for cell in candidates {
        if cell_distance(cell, crater.center) > radius {
            continue;
        }
        let old_capacity = old_geometry
            .capacity_m3(cell)
            .ok_or(FluidError::Unavailable)?;
        let new_capacity = new_geometry
            .capacity_m3(cell)
            .ok_or(FluidError::Unavailable)?;
        if (new_capacity - old_capacity).abs() <= 1.0e-9 {
            continue;
        }
        changes.push((cell, old_capacity, new_capacity));
    }

    // Grow and activate receiving space first so shrink displacement can use it.
    for (cell, old_capacity, new_capacity) in &changes {
        if new_capacity <= old_capacity {
            continue;
        }
        if staged.cells().contains_key(cell) {
            staged.set_capacity(*cell, *new_capacity)?;
        } else if *new_capacity > 0.0 {
            owner.activate_with_external_water(&mut staged, *cell, *new_capacity)?;
        }
        check_budget(owner, &staged, server_cell_limit)?;
    }

    // Apply all shrink displacements against proposed apertures and capacities.
    for (cell, old_capacity, new_capacity) in changes {
        if new_capacity >= old_capacity || !staged.cells().contains_key(&cell) {
            continue;
        }
        let change = staged.prepare_geometry_change(&new_geometry, cell, new_capacity)?;
        staged.apply_geometry_change(change)?;
        check_budget(owner, &staged, server_cell_limit)?;
    }

    super::frontier::activate_frontier(owner, &mut staged, &new_geometry, [0.0; 3], 9.81, 0.1)?;
    Ok(PreparedWaterEdit { snapshot: staged })
}

fn check_budget(
    owner: &WaterWorld,
    snapshot: &minimium_fluids::FluidSnapshot,
    server_cell_limit: usize,
) -> Result<(), FluidError> {
    if snapshot.cells().len() > owner.reserved_cell_limit
        || snapshot.cells().len() > server_cell_limit
    {
        Err(FluidError::Capacity)
    } else {
        Ok(())
    }
}

fn cell_distance(cell: [i32; 3], point: [f64; 3]) -> f64 {
    let center = cell.map(|value| f64::from(value) * CELL_SIZE_M + CELL_SIZE_M * 0.5);
    center
        .iter()
        .zip(point)
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f64>()
        .sqrt()
}
