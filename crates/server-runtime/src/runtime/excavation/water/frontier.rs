use super::WaterWorld;
use minimium_fluids::{
    FluidError, FluidGeometry, FluidSnapshot, TerrainFluidGeometry, WaterEndpoint, CELL_SIZE_M,
};
use std::collections::BTreeSet;

const MAX_FRONTIER_ACTIVATIONS: usize = 4096;
const MAX_FRONTIER_LAYERS: usize = 4096;

pub(super) fn activate_frontier(
    owner: &WaterWorld,
    staged: &mut FluidSnapshot,
    geometry: &TerrainFluidGeometry<'_>,
    center_m: [f64; 3],
    gravity_m_s2: f64,
    dt: f64,
) -> Result<usize, FluidError> {
    if !dt.is_finite() || dt <= 0.0 || !gravity_m_s2.is_finite() || gravity_m_s2 < 0.0 {
        return Err(FluidError::Invalid);
    }
    let mut seeds = BTreeSet::new();
    let mut max_face_speed = 0.0_f64;
    let mut max_reservoir_head = 0.0_f64;
    for (key, cell) in staged.cells() {
        let wet = cell.volume_m3 > 1.0e-12;
        if !wet && cell.capacity_m3 <= 0.0 {
            continue;
        }
        let interface = wet && cell.volume_m3 < cell.capacity_m3 - 1.0e-12;
        let moving = cell
            .positive_velocity_m_s
            .iter()
            .chain(cell.negative_boundary_velocity_m_s.iter())
            .fold(0.0_f64, |maximum, velocity| maximum.max(velocity.abs()));
        max_face_speed = max_face_speed.max(moving);
        let mut exposed_frontier = false;
        let mut reservoir_inflow = false;
        for axis in 0..3 {
            for sign in [-1, 1] {
                let face = geometry
                    .face(*key, axis, sign > 0)
                    .ok_or(FluidError::Unavailable)?;
                if face.open_area_m2 <= 0.0 {
                    continue;
                }
                let mut neighbor = *key;
                let Some(coordinate) = neighbor[axis].checked_add(sign) else {
                    continue;
                };
                neighbor[axis] = coordinate;
                let mut face_point =
                    key.map(|value| f64::from(value) * CELL_SIZE_M + CELL_SIZE_M * 0.5);
                face_point[axis] += if sign > 0 {
                    CELL_SIZE_M * 0.5
                } else {
                    -CELL_SIZE_M * 0.5
                };
                let external_body = if !staged.cells().contains_key(&neighbor) {
                    owner.external_water_body_at_point(staged, face_point, center_m)
                } else {
                    None
                };
                if let Some(id) = external_body {
                    if let Some(level) = staged
                        .reservoirs()
                        .get(&id)
                        .and_then(|reservoir| reservoir.level_m().ok())
                    {
                        let radial = std::array::from_fn::<_, 3, _>(|index| {
                            face_point[index] - center_m[index]
                        });
                        let radius = radial.iter().map(|value| value * value).sum::<f64>().sqrt();
                        max_reservoir_head = max_reservoir_head.max((level - radius).max(0.0));
                    }
                    reservoir_inflow = true;
                    continue;
                }
                if let WaterEndpoint::Cell(next) = face.boundary {
                    match staged.cells().get(&next) {
                        None if wet => exposed_frontier = true,
                        Some(neighbor)
                            if wet
                                && neighbor.capacity_m3 > 0.0
                                && neighbor.volume_m3 <= 1.0e-12 =>
                        {
                            exposed_frontier = true;
                        }
                        _ => {}
                    }
                }
            }
        }
        if interface || moving > 1.0e-5 || exposed_frontier || reservoir_inflow {
            seeds.insert(*key);
        }
    }
    if seeds.is_empty() {
        return Ok(0);
    }

    let wave_speed = (2.0 * gravity_m_s2 * max_reservoir_head).sqrt();
    let speed_bound = max_face_speed + gravity_m_s2 * dt + wave_speed;
    // Keep the activation halo at least as wide as the solver's 0.5-cell CFL
    // travel bound, then add one layer for pressure-driven arrival.
    let estimated_layers = speed_bound * dt / (0.5 * CELL_SIZE_M);
    if !estimated_layers.is_finite() || estimated_layers >= MAX_FRONTIER_LAYERS as f64 {
        return Err(FluidError::Capacity);
    }
    let layers = estimated_layers.ceil() as usize + 1;
    let available = owner
        .reserved_cell_limit
        .saturating_sub(staged.cells().len());
    let activation_limit = MAX_FRONTIER_ACTIVATIONS.min(available);
    let mut candidates = BTreeSet::new();
    let mut visited = seeds.clone();
    let mut frontier = seeds;
    for _ in 0..layers {
        let mut next_frontier = BTreeSet::new();
        for key in frontier {
            for axis in 0..3 {
                for sign in [-1, 1] {
                    let face = geometry
                        .face(key, axis, sign > 0)
                        .ok_or(FluidError::Unavailable)?;
                    if face.open_area_m2 <= 0.0 {
                        continue;
                    }
                    let mut face_point =
                        key.map(|value| f64::from(value) * CELL_SIZE_M + CELL_SIZE_M * 0.5);
                    face_point[axis] += if sign > 0 {
                        CELL_SIZE_M * 0.5
                    } else {
                        -CELL_SIZE_M * 0.5
                    };
                    if matches!(face.boundary, WaterEndpoint::Reservoir(_))
                        || owner
                            .external_water_body_at_point(staged, face_point, center_m)
                            .is_some()
                    {
                        continue;
                    }
                    let WaterEndpoint::Cell(next) = face.boundary else {
                        continue;
                    };
                    if !visited.insert(next) {
                        continue;
                    }
                    if !staged.cells().contains_key(&next) {
                        candidates.insert(next);
                        if candidates.len() > activation_limit {
                            return Err(FluidError::Capacity);
                        }
                    }
                    next_frontier.insert(next);
                }
            }
        }
        if next_frontier.is_empty() {
            break;
        }
        frontier = next_frontier;
    }

    let mut activated = 0;
    for key in candidates {
        let capacity = geometry.capacity_m3(key).ok_or(FluidError::Unavailable)?;
        if capacity > 0.0 {
            owner.activate_with_external_water(staged, key, capacity)?;
            activated += 1;
        }
    }
    Ok(activated)
}
