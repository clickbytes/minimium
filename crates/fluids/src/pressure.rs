use crate::{CellWater, FluidError, FluidGeometry, FluidSnapshot, WaterEndpoint, CELL_SIZE_M};
use std::collections::BTreeMap;

/// Jacobi projection for the active MAC lattice. Reservoirs are pressure
/// Dirichlet boundaries at their level; solids have zero aperture.
pub(crate) fn project<G: FluidGeometry>(
    snapshot: &FluidSnapshot,
    geometry: &G,
    dt: f64,
    radial_center: Option<[f64; 3]>,
    gravity_at_cell: &impl Fn([i32; 3]) -> [f64; 3],
) -> Result<BTreeMap<[i32; 3], ([f64; 3], [f64; 3])>, FluidError> {
    let keys: Vec<_> = snapshot.cells().keys().copied().collect();
    let wet_keys: Vec<_> = keys
        .iter()
        .copied()
        .filter(|key| pressurized(&snapshot.cells()[key]))
        .collect();
    let mut divergence = BTreeMap::new();
    for key in &wet_keys {
        let cell = snapshot.cells().get(key).ok_or(FluidError::Unavailable)?;
        let mut div = 0.0;
        for axis in 0..3 {
            let positive = geometry
                .face(*key, axis, true)
                .ok_or(FluidError::Unavailable)?;
            div += cell.positive_velocity_m_s[axis] * positive.open_area_m2;
            let negative = geometry
                .face(*key, axis, false)
                .ok_or(FluidError::Unavailable)?;
            let negative_velocity = match negative.boundary {
                WaterEndpoint::Cell(prev) => {
                    snapshot
                        .cells()
                        .get(&prev)
                        .ok_or(FluidError::Unavailable)?
                        .positive_velocity_m_s[axis]
                }
                _ => cell.negative_boundary_velocity_m_s[axis],
            };
            div -= negative_velocity * negative.open_area_m2;
        }
        let volume = cell.capacity_m3.max(1.0e-9);
        divergence.insert(*key, div / volume);
    }
    // Dry active cells represent air and stay at atmospheric pressure.
    let mut pressure: BTreeMap<_, _> = keys.iter().map(|key| (*key, 0.0)).collect();
    let mut next = pressure.clone();
    for _ in 0..32 {
        for key in &wet_keys {
            let mut weighted = 0.0;
            let mut total = 0.0;
            for axis in 0..3 {
                for positive in [false, true] {
                    let face = geometry
                        .face(*key, axis, positive)
                        .ok_or(FluidError::Unavailable)?;
                    if face.open_area_m2 <= 0.0 {
                        continue;
                    }
                    let boundary_pressure = match face.boundary {
                        WaterEndpoint::Cell(other) => {
                            let neighbor = snapshot
                                .cells()
                                .get(&other)
                                .ok_or(FluidError::Unavailable)?;
                            if pressurized(neighbor) {
                                *pressure.get(&other).ok_or(FluidError::Unavailable)?
                            } else {
                                0.0
                            }
                        }
                        WaterEndpoint::Reservoir(id) => reservoir_pressure(
                            snapshot,
                            id,
                            *key,
                            axis,
                            positive,
                            radial_center,
                            gravity_at_cell,
                        )?,
                        WaterEndpoint::Air => 0.0,
                        WaterEndpoint::Solid => continue,
                    };
                    weighted += boundary_pressure * face.open_area_m2;
                    total += face.open_area_m2;
                }
            }
            if total > 0.0 {
                let source = divergence[key] * snapshot.cells()[key].capacity_m3 * CELL_SIZE_M / dt;
                next.insert(*key, (weighted - source) / total);
            }
        }
        std::mem::swap(&mut pressure, &mut next);
    }
    let mut velocity = BTreeMap::new();
    for key in keys {
        let source = *pressure.get(&key).ok_or(FluidError::Unavailable)?;
        let mut positive = snapshot.cells()[&key].positive_velocity_m_s;
        let mut negative = snapshot.cells()[&key].negative_boundary_velocity_m_s;
        for axis in 0..3 {
            let face = geometry
                .face(key, axis, true)
                .ok_or(FluidError::Unavailable)?;
            if face.open_area_m2 <= 0.0 {
                positive[axis] = 0.0;
            } else {
                let dest = match face.boundary {
                    WaterEndpoint::Cell(other) => {
                        let neighbor = snapshot
                            .cells()
                            .get(&other)
                            .ok_or(FluidError::Unavailable)?;
                        if pressurized(neighbor) {
                            *pressure.get(&other).ok_or(FluidError::Unavailable)?
                        } else {
                            0.0
                        }
                    }
                    WaterEndpoint::Reservoir(id) => reservoir_pressure(
                        snapshot,
                        id,
                        key,
                        axis,
                        true,
                        radial_center,
                        gravity_at_cell,
                    )?,
                    WaterEndpoint::Air => 0.0,
                    WaterEndpoint::Solid => 0.0,
                };
                if matches!(face.boundary, WaterEndpoint::Solid | WaterEndpoint::Air) {
                    positive[axis] = 0.0;
                } else {
                    positive[axis] -= dt * (dest - source) / CELL_SIZE_M;
                }
            }
            let face = geometry
                .face(key, axis, false)
                .ok_or(FluidError::Unavailable)?;
            if face.open_area_m2 <= 0.0 {
                negative[axis] = 0.0;
            } else if !matches!(face.boundary, WaterEndpoint::Cell(_)) {
                let outside = match face.boundary {
                    WaterEndpoint::Reservoir(id) => reservoir_pressure(
                        snapshot,
                        id,
                        key,
                        axis,
                        false,
                        radial_center,
                        gravity_at_cell,
                    )?,
                    WaterEndpoint::Air => 0.0,
                    _ => 0.0,
                };
                negative[axis] -= dt * (source - outside) / CELL_SIZE_M;
                if matches!(face.boundary, WaterEndpoint::Air) {
                    negative[axis] = 0.0;
                }
            } else {
                negative[axis] = 0.0;
            }
        }
        velocity.insert(key, (positive, negative));
    }
    Ok(velocity)
}

fn pressurized(cell: &CellWater) -> bool {
    cell.capacity_m3 > 0.0 && cell.volume_m3 >= cell.capacity_m3 - 1.0e-10
}

fn reservoir_pressure(
    snapshot: &FluidSnapshot,
    id: minimium_world::fluid::WaterBodyId,
    cell: [i32; 3],
    axis: usize,
    positive: bool,
    center: Option<[f64; 3]>,
    gravity_at_cell: &impl Fn([i32; 3]) -> [f64; 3],
) -> Result<f64, FluidError> {
    let level = snapshot
        .reservoirs()
        .get(&id)
        .ok_or(FluidError::Unavailable)?
        .level_m()?;
    let Some(center) = center else {
        return Ok(0.0);
    };
    let mut point = cell.map(|v| f64::from(v) * CELL_SIZE_M + CELL_SIZE_M * 0.5);
    point[axis] += if positive {
        CELL_SIZE_M * 0.5
    } else {
        -CELL_SIZE_M * 0.5
    };
    let radius = point
        .iter()
        .zip(center)
        .map(|(p, c)| (p - c).powi(2))
        .sum::<f64>()
        .sqrt();
    let gravity = gravity_at_cell(cell);
    let magnitude = gravity.iter().map(|v| v * v).sum::<f64>().sqrt();
    Ok(magnitude * (level - radius).max(0.0))
}
