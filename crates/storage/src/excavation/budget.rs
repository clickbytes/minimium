use super::*;

pub(super) fn checkpoint_size(
    state: &ExcavationRestore,
    mutation: &WorldMutation,
) -> Result<usize, WorldChangeFailure> {
    let mut bytes = 32usize;
    let mut world_count = state.worlds.len();
    if !state.worlds.contains_key(&mutation.world) {
        world_count += 1;
        bytes += 10;
    }
    bytes = bytes
        .checked_add(
            world_count
                .checked_mul(44)
                .ok_or(WorldChangeFailure::LimitExceeded)?,
        )
        .ok_or(WorldChangeFailure::LimitExceeded)?;
    for world in state.worlds.values() {
        let chunk_bytes = world
            .chunks
            .len()
            .checked_mul(20 + SAMPLES_PER_CHUNK * 9)
            .ok_or(WorldChangeFailure::LimitExceeded)?;
        bytes = bytes
            .checked_add(chunk_bytes)
            .ok_or(WorldChangeFailure::LimitExceeded)?;
        bytes = bytes
            .checked_add(2)
            .ok_or(WorldChangeFailure::LimitExceeded)?;
        if let Some(fluid) = &world.fluids {
            bytes = bytes
                .checked_add(fluid_size(fluid)?)
                .ok_or(WorldChangeFailure::LimitExceeded)?;
        }
        if world.water_catalog_digest.is_some() {
            bytes = bytes
                .checked_add(32)
                .ok_or(WorldChangeFailure::LimitExceeded)?;
        }
        let body_bytes = 4 + world.impact_bodies.len() * minimium_world::impact::BODY_RECORD_BYTES;
        bytes = bytes
            .checked_add(body_bytes)
            .ok_or(WorldChangeFailure::LimitExceeded)?;
        bytes = bytes
            .checked_add(4 + world.craters.len() * 56)
            .ok_or(WorldChangeFailure::LimitExceeded)?;
    }

    let existing = state.worlds.get(&mutation.world);
    let added_chunks = mutation
        .resulting_chunks
        .iter()
        .filter(|chunk| {
            existing.is_none_or(|world| !world.chunks.contains_key(&chunk.key().coordinates))
        })
        .count();
    let added_chunk_bytes = added_chunks
        .checked_mul(20 + SAMPLES_PER_CHUNK * 9)
        .ok_or(WorldChangeFailure::LimitExceeded)?;
    bytes = bytes
        .checked_add(added_chunk_bytes)
        .ok_or(WorldChangeFailure::LimitExceeded)?;

    if let Some(fluid) = &mutation.fluids {
        if let Some(previous) = existing.and_then(|world| world.fluids.as_ref()) {
            bytes = bytes.saturating_sub(fluid_size(previous)?);
        }
        bytes = bytes
            .checked_add(fluid_size(fluid)?)
            .ok_or(WorldChangeFailure::LimitExceeded)?;
    }
    if !mutation.impact_bodies.is_empty() {
        if let Some(world) = existing {
            bytes = bytes.saturating_sub(
                world.impact_bodies.len() * minimium_world::impact::BODY_RECORD_BYTES,
            );
        }
        let new_body_count = mutation
            .impact_bodies
            .iter()
            .filter(|body| existing.is_none_or(|world| !world.impact_bodies.contains_key(&body.id)))
            .count();
        let total_bodies = existing.map_or(0, |world| world.impact_bodies.len()) + new_body_count;
        bytes = bytes
            .checked_add(total_bodies * minimium_world::impact::BODY_RECORD_BYTES)
            .ok_or(WorldChangeFailure::LimitExceeded)?;
    }
    if !mutation.craters.is_empty() {
        if let Some(world) = existing {
            bytes = bytes.saturating_sub(world.craters.len() * 56);
        }
        let added_craters = mutation
            .craters
            .iter()
            .filter(|crater| existing.is_none_or(|world| !world.craters.contains_key(&crater.id)))
            .count();
        let total_craters = existing.map_or(0, |world| world.craters.len()) + added_craters;
        bytes = bytes
            .checked_add(total_craters * 56)
            .ok_or(WorldChangeFailure::LimitExceeded)?;
    }
    if mutation.water_catalog_digest.is_some()
        && existing
            .and_then(|world| world.water_catalog_digest)
            .is_none()
    {
        bytes = bytes
            .checked_add(32)
            .ok_or(WorldChangeFailure::LimitExceeded)?;
    }
    if let Some((profile, _)) = mutation.inventory {
        let extra_inventory = if state.inventories.contains_key(&profile) {
            0
        } else {
            16 + 11
        };
        bytes = bytes
            .checked_add(extra_inventory)
            .ok_or(WorldChangeFailure::LimitExceeded)?;
    }
    bytes = bytes
        .checked_add(20)
        .ok_or(WorldChangeFailure::LimitExceeded)?;
    Ok(bytes)
}

pub(super) fn fluid_size(fluid: &FluidSnapshot) -> Result<usize, WorldChangeFailure> {
    let cell_bytes = fluid
        .cells()
        .len()
        .checked_mul(76)
        .ok_or(WorldChangeFailure::LimitExceeded)?;
    let mut bytes = 20usize
        .checked_add(cell_bytes)
        .ok_or(WorldChangeFailure::LimitExceeded)?;
    for reservoir in fluid.reservoirs().values() {
        let size = match &reservoir.kind {
            ReservoirKind::Ocean { .. } => 33,
            ReservoirKind::Finite { curve, .. } => {
                let points_size = curve
                    .len()
                    .checked_mul(16)
                    .ok_or(WorldChangeFailure::LimitExceeded)?;
                27usize
                    .checked_add(points_size)
                    .ok_or(WorldChangeFailure::LimitExceeded)?
            }
        };
        bytes = bytes
            .checked_add(size)
            .ok_or(WorldChangeFailure::LimitExceeded)?;
    }
    Ok(bytes)
}
