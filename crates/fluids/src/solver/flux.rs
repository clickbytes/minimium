use crate::{FluidError, FluidSnapshot, WaterEndpoint};
use std::collections::BTreeMap;

pub(super) fn face_point(cell: [i32; 3], axis: usize, positive: bool) -> [f64; 3] {
    std::array::from_fn(|component| {
        f64::from(cell[component]) * crate::CELL_SIZE_M
            + crate::CELL_SIZE_M
                * if component != axis {
                    0.5
                } else if positive {
                    1.0
                } else {
                    0.0
                }
    })
}

pub(super) fn endpoint_wet(
    snapshot: &FluidSnapshot,
    endpoint: WaterEndpoint,
    point: [f64; 3],
    radial_center: Option<[f64; 3]>,
) -> Result<bool, FluidError> {
    match endpoint {
        WaterEndpoint::Cell(key) => Ok(snapshot
            .cells()
            .get(&key)
            .ok_or(FluidError::Unavailable)?
            .volume_m3
            > 1.0e-12),
        WaterEndpoint::Reservoir(id) => {
            let reservoir = snapshot
                .reservoirs()
                .get(&id)
                .ok_or(FluidError::Unavailable)?;
            if reservoir.available() <= 1.0e-12 {
                return Ok(false);
            }
            if let Some(center) = radial_center {
                let radius = point
                    .iter()
                    .zip(center)
                    .map(|(coordinate, origin)| (coordinate - origin).powi(2))
                    .sum::<f64>()
                    .sqrt();
                return Ok(reservoir.level_m()? >= radius);
            }
            Ok(true)
        }
        WaterEndpoint::Solid | WaterEndpoint::Air => Ok(false),
    }
}

pub(super) fn limit_reservoir_outflows(
    snapshot: &FluidSnapshot,
    fluxes: &mut Vec<(WaterEndpoint, WaterEndpoint, f64)>,
) -> Result<(), FluidError> {
    let mut groups = BTreeMap::<_, Vec<usize>>::new();
    for (index, (from, _, _)) in fluxes.iter().enumerate() {
        if let WaterEndpoint::Reservoir(id) = from {
            groups.entry(*id).or_default().push(index);
        }
    }
    let mut remove = Vec::new();
    for (id, indices) in groups {
        let available = snapshot
            .reservoirs()
            .get(&id)
            .ok_or(FluidError::Unavailable)?
            .available();
        if !available.is_finite() {
            continue;
        }
        let requested: f64 = indices.iter().map(|index| fluxes[*index].2).sum();
        if requested <= available {
            continue;
        }
        let scale = available / requested;
        let mut allocated = 0.0;
        for (position, index) in indices.iter().copied().enumerate() {
            let amount = if position + 1 == indices.len() {
                (available - allocated).max(0.0).min(fluxes[index].2)
            } else {
                let amount = fluxes[index].2 * scale;
                allocated += amount;
                amount
            };
            fluxes[index].2 = amount;
            if amount == 0.0 {
                remove.push(index);
            }
        }
    }
    for index in remove.into_iter().rev() {
        fluxes.remove(index);
    }
    Ok(())
}
