use super::*;
use crate::{FluidGeometry, WaterEndpoint};

pub(super) fn prepare<G: FluidGeometry>(
    snapshot: &FluidSnapshot,
    geometry: &G,
    cell: [i32; 3],
    new_capacity_m3: f64,
) -> Result<GeometryChange, FluidError> {
    if !(0.0..=CELL_VOLUME_M3).contains(&new_capacity_m3) {
        return Err(FluidError::Invalid);
    }
    let old = geometry.capacity_m3(cell).ok_or(FluidError::Unavailable)?;
    let occupied = snapshot.cells().get(&cell).map_or(0.0, |c| c.volume_m3);
    let displaced = (occupied - new_capacity_m3).max(0.0);
    let mut queue = vec![cell];
    let mut seen = std::collections::BTreeSet::from([cell]);
    let mut receivers = Vec::new();
    let mut available = 0.0;
    let mut cursor = 0;
    while cursor < queue.len() && available < displaced {
        let current = queue[cursor];
        cursor += 1;
        for axis in 0..3 {
            for positive in [false, true] {
                let face = geometry
                    .face(current, axis, positive)
                    .ok_or(FluidError::Unavailable)?;
                if face.open_area_m2 <= 0.0 {
                    continue;
                }
                let WaterEndpoint::Cell(next) = face.boundary else {
                    continue;
                };
                if !seen.insert(next) {
                    continue;
                }
                if let Some(target) = snapshot.cells().get(&next) {
                    queue.push(next);
                    let free = target.capacity_m3 - target.volume_m3;
                    if free > 0.0 {
                        receivers.push((next, free));
                        available += free;
                    }
                }
            }
        }
    }
    if displaced > available + f64::EPSILON {
        return Err(FluidError::DisplacementRequired);
    }
    Ok(GeometryChange {
        cell,
        old_capacity_m3: old,
        new_capacity_m3,
        displaced_m3: displaced,
        receivers,
    })
}

pub(super) fn apply(
    snapshot: &mut FluidSnapshot,
    change: GeometryChange,
) -> Result<(), FluidError> {
    if !snapshot.cells().contains_key(&change.cell) {
        if change.displaced_m3 > 0.0 {
            return Err(FluidError::Unavailable);
        }
        return snapshot.activate(change.cell, change.new_capacity_m3);
    }
    let cell = snapshot
        .cells()
        .get(&change.cell)
        .copied()
        .ok_or(FluidError::Unavailable)?;
    let displaced = (cell.volume_m3 - change.new_capacity_m3).max(0.0);
    if displaced > change.displaced_m3 {
        return Err(FluidError::DisplacementRequired);
    }
    let mut plan = Vec::new();
    let mut remaining = displaced;
    for (key, free) in &change.receivers {
        let current_free = snapshot
            .cells()
            .get(key)
            .ok_or(FluidError::Unavailable)?
            .capacity_m3
            - snapshot.cells()[key].volume_m3;
        let amount = remaining.min((*free).min(current_free));
        if amount > 0.0 {
            plan.push((*key, amount));
            remaining -= amount;
        }
        if remaining <= f64::EPSILON {
            break;
        }
    }
    if remaining > f64::EPSILON {
        return Err(FluidError::DisplacementRequired);
    }
    // Checkpoint is not mutated until all receivers have been reserved.
    for (key, amount) in plan {
        snapshot.transfer(
            WaterEndpoint::Cell(change.cell),
            WaterEndpoint::Cell(key),
            amount,
        )?;
    }
    snapshot.set_capacity(change.cell, change.new_capacity_m3)
}
