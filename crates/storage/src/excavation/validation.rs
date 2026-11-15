use super::*;

pub(super) fn validate_mutation(
    mutation: &WorldMutation,
    state: &ExcavationRestore,
) -> io::Result<()> {
    if mutation.id == 0
        || mutation.id <= state.last_mutation_id
        || mutation.resulting_chunks.len() > 64
        || mutation.impact_bodies.len() > 4096
        || (mutation.resulting_chunks.is_empty()
            && mutation.inventory.is_none()
            && mutation.fluids.is_none()
            && mutation.impact_bodies.is_empty()
            && mutation.water_catalog_digest.is_none())
    {
        return Err(invalid("invalid excavation mutation identity or size"));
    }
    if mutation
        .fluids
        .as_ref()
        .is_some_and(|fluid| fluid.world() != mutation.world)
    {
        return Err(invalid("fluid checkpoint world mismatch"));
    }

    let world = state.worlds.get(&mutation.world);
    if world.is_some_and(|saved| saved.world_seed != mutation.world_seed) {
        return Err(invalid("world seed changed"));
    }
    if mutation.water_catalog_digest.is_some_and(|digest| {
        world
            .and_then(|saved| saved.water_catalog_digest)
            .is_some_and(|old| old != digest)
    }) {
        return Err(invalid("water catalog digest changed"));
    }
    if (!mutation.resulting_chunks.is_empty() || mutation.fluids.is_some())
        && mutation.water_catalog_digest.is_none()
        && world.and_then(|saved| saved.water_catalog_digest).is_none()
    {
        return Err(invalid("geometry checkpoint lacks water catalog digest"));
    }

    let mut expected_by_chunk = BTreeMap::new();
    for (key, expected) in &mutation.expected_chunk_revisions {
        if key.world != mutation.world
            || expected_by_chunk
                .insert(key.coordinates, expected.0)
                .is_some()
        {
            return Err(invalid("invalid expected chunk revisions"));
        }
        let current = world
            .and_then(|saved| saved.chunks.get(&key.coordinates))
            .map_or(0, |chunk| chunk.revision().0);
        if current != expected.0 {
            return Err(invalid("chunk revision conflict"));
        }
    }

    let mut seen_chunks = HashSet::new();
    for chunk in &mutation.resulting_chunks {
        let key = chunk.key();
        let Some(expected) = expected_by_chunk.get(&key.coordinates).copied() else {
            return Err(invalid("resulting chunk has no expected revision"));
        };
        let next_revision = expected
            .checked_add(1)
            .ok_or_else(|| invalid("chunk revision exhausted"))?;
        if key.world != mutation.world
            || chunk.revision().0 != next_revision
            || !seen_chunks.insert(key.coordinates)
        {
            return Err(invalid("invalid resulting chunk revision"));
        }
    }
    if seen_chunks.len() != expected_by_chunk.len() {
        return Err(invalid("expected revision has no resulting chunk"));
    }

    if let Some((_, inventory)) = mutation.inventory {
        if inventory
            .equipped
            .is_some_and(|slot| slot >= 2 || inventory.slots[usize::from(slot)].is_none())
            || inventory
                .slots
                .iter()
                .flatten()
                .any(|item| !matches!(item.kind, ToolKind::Shovel | ToolKind::Pickaxe))
        {
            return Err(invalid("invalid inventory"));
        }
    }
    if let Some((profile, inventory)) = mutation.inventory {
        let previous = state
            .inventories
            .get(&profile)
            .map_or(0, |saved| saved.revision);
        let next_revision = previous
            .checked_add(1)
            .ok_or_else(|| invalid("inventory revision exhausted"))?;
        if inventory.revision != next_revision {
            return Err(invalid("inventory revision conflict"));
        }
    }
    if let Some(fluid) = &mutation.fluids {
        let previous = world
            .and_then(|saved| saved.fluids.as_ref())
            .map(|saved| saved.revision());
        if previous.is_some_and(|revision| fluid.revision() < revision) {
            return Err(invalid("fluid snapshot revision conflict"));
        }
    }

    let mut body_ids = HashSet::new();
    for body in &mutation.impact_bodies {
        if !body.valid()
            || body.world != mutation.world
            || body.world_seed != mutation.world_seed
            || !body_ids.insert(body.id)
            || world
                .and_then(|saved| saved.impact_bodies.get(&body.id))
                .is_some_and(|old| !body.follows(old))
        {
            return Err(invalid("invalid or regressed impact body"));
        }
        if body.phase != 0 && body.crater().is_none_or(|crater| !crater.valid()) {
            return Err(invalid("invalid committed crater"));
        }
    }
    if world.is_some_and(|saved| {
        saved.impact_bodies.len()
            + mutation
                .impact_bodies
                .iter()
                .filter(|body| !saved.impact_bodies.contains_key(&body.id))
                .count()
            > 4096
    }) {
        return Err(invalid("impact body capacity exceeded"));
    }
    if mutation.craters.len() > minimium_world::terrain_edits::MAX_TERRAIN_EDITS {
        return Err(invalid("crater capacity exceeded"));
    }
    let mut crater_ids = HashSet::new();
    for crater in &mutation.craters {
        if !crater.valid()
            || !crater_ids.insert(crater.id)
            || world
                .and_then(|saved| saved.craters.get(&crater.id))
                .is_some_and(|old| old != crater)
        {
            return Err(invalid("invalid or changed crater stamp"));
        }
    }
    for body in &mutation.impact_bodies {
        if body.phase != 0
            && world
                .and_then(|saved| saved.craters.get(&body.id))
                .is_none()
            && !crater_ids.contains(&body.id)
        {
            return Err(invalid("committed impact lacks persisted crater stamp"));
        }
    }
    if mutation.craters.iter().any(|crater| {
        world
            .and_then(|saved| saved.impact_bodies.get(&crater.id))
            .is_none_or(|body| body.phase == 0)
            && !mutation
                .impact_bodies
                .iter()
                .any(|body| body.id == crater.id && body.phase != 0)
    }) {
        return Err(invalid("crater has no committed impact body"));
    }
    Ok(())
}
