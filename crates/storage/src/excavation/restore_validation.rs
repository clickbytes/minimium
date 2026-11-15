use super::*;

/// Validate injected restores with the same structural decoder and byte limits
/// as the file backend, before the runtime installs any restored state.
pub fn validate_restore(state: &ExcavationRestore) -> io::Result<()> {
    checked_checkpoint(state, MAX_JOURNAL_BYTES as usize).map(|_| ())
}

pub(super) fn checked_checkpoint(state: &ExcavationRestore, maximum: usize) -> io::Result<Vec<u8>> {
    if state.worlds.len() > 16_384 || state.inventories.len() > 1_000_000 {
        return Err(invalid("excavation restore count exceeds limit"));
    }
    if state.inventories.values().any(|inventory| {
        inventory
            .equipped
            .is_some_and(|slot| slot >= 2 || inventory.slots[usize::from(slot)].is_none())
    }) {
        return Err(invalid("invalid restored inventory"));
    }
    // Exact encoded sizes, checked before allocating the checkpoint image.
    let mut size = 32usize;
    let mut add = |bytes: usize| -> io::Result<()> {
        size = size
            .checked_add(bytes)
            .ok_or_else(|| invalid("restore size overflow"))?;
        if size > maximum {
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "excavation restore byte budget exceeded",
            ));
        }
        Ok(())
    };
    add(state.inventories.len() * 27)?;
    for (id, world) in &state.worlds {
        if world.chunks.len() > 65_536
            || world.impact_bodies.len() > 4096
            || world.craters.len() > minimium_world::terrain_edits::MAX_TERRAIN_EDITS
            || world.last_mutation > state.last_mutation_id
            || world.chunks.iter().any(|(coordinates, chunk)| {
                chunk.key().world != *id || chunk.key().coordinates != *coordinates
            })
            || world
                .impact_bodies
                .iter()
                .any(|(key, body)| *key != body.id || body.world != *id)
            || world.craters.iter().any(|(key, crater)| *key != crater.id)
            || world
                .fluids
                .as_ref()
                .is_some_and(|fluid| fluid.world() != *id)
        {
            return Err(invalid("invalid excavation restore identity or count"));
        }
        add(54)?;
        if world.water_catalog_digest.is_some() {
            add(32)?;
        }
        add(world.chunks.len() * (20 + SAMPLES_PER_CHUNK * 9))?;
        add(world.impact_bodies.len() * minimium_world::impact::BODY_RECORD_BYTES)?;
        add(world.craters.len() * 56)?;
        if let Some(fluid) = &world.fluids {
            add(budget::fluid_size(fluid)?)?;
        }
    }
    let bytes = encode_checkpoint(state)?;
    if bytes.len() > maximum {
        return Err(io::Error::new(
            io::ErrorKind::FileTooLarge,
            "excavation checkpoint byte budget exceeded",
        ));
    }
    decode_checkpoint(&bytes[CHECKPOINT.len()..])?;
    Ok(bytes)
}
