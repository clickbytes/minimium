use super::*;

pub(crate) fn encode_checkpoint(state: &ExcavationRestore) -> io::Result<Vec<u8>> {
    let mut out = CHECKPOINT.to_vec();
    put_u64(&mut out, state.last_mutation_id);
    put_u64(&mut out, state.mutation_count);
    put_u32(&mut out, state.worlds.len() as u32);
    for (world, restored) in &state.worlds {
        put_u128(&mut out, world.get());
        put_u128(&mut out, restored.world_seed);
        put_u64(&mut out, restored.last_mutation);
        put_u32(&mut out, restored.chunks.len() as u32);
        for chunk in restored.chunks.values() {
            put_chunk(&mut out, chunk)?;
        }
        match &restored.fluids {
            None => out.push(0),
            Some(fluid) => {
                out.push(1);
                put_fluid(&mut out, fluid)?;
            }
        }
        put_u32(&mut out, restored.impact_bodies.len() as u32);
        for body in restored.impact_bodies.values() {
            out.extend_from_slice(&body.encode());
        }
        put_u32(&mut out, restored.craters.len() as u32);
        for crater in restored.craters.values() {
            put_crater(&mut out, crater);
        }
        match restored.water_catalog_digest {
            None => out.push(0),
            Some(digest) => {
                out.push(1);
                out.extend_from_slice(&digest);
            }
        }
    }
    put_u32(&mut out, state.inventories.len() as u32);
    let mut profiles: Vec<_> = state.inventories.iter().collect();
    profiles.sort_by_key(|(profile, _)| profile.as_bytes().to_owned());
    for (profile, inventory) in profiles {
        out.extend_from_slice(profile.as_bytes());
        put_inventory(&mut out, *inventory);
    }
    Ok(out)
}

pub(crate) fn decode_checkpoint(bytes: &[u8]) -> io::Result<ExcavationRestore> {
    let mut reader = Decode { b: bytes, p: 0 };
    let mut state = ExcavationRestore::default();
    state.last_mutation_id = reader.u64()?;
    state.mutation_count = reader.u64()?;
    let world_count = reader.u32()? as usize;
    if world_count > 16_384 {
        return Err(invalid("checkpoint world count exceeds limit"));
    }
    for _ in 0..world_count {
        let id = WorldInstanceId::new(reader.u128()?)
            .ok_or_else(|| invalid("invalid checkpoint world"))?;
        let seed = reader.u128()?;
        let last_mutation = reader.u64()?;
        if last_mutation > state.last_mutation_id {
            return Err(invalid("checkpoint world revision is ahead of journal"));
        }
        let count = reader.u32()? as usize;
        if count > 65_536 {
            return Err(invalid("checkpoint chunk count exceeds limit"));
        }
        let mut chunks = BTreeMap::new();
        for _ in 0..count {
            let chunk = Arc::new(reader.chunk(id)?);
            if chunks.insert(chunk.key().coordinates, chunk).is_some() {
                return Err(invalid("duplicate checkpoint chunk"));
            }
        }
        let fluids = match reader.byte()? {
            0 => None,
            1 => Some(reader.fluid(id)?),
            _ => return Err(invalid("invalid checkpoint fluid flag")),
        };
        let count = reader.u32()? as usize;
        if count > 4096 {
            return Err(invalid("too many checkpoint impact bodies"));
        }
        let mut impact_bodies = BTreeMap::new();
        for _ in 0..count {
            let body = reader.body()?;
            if body.world != id
                || body.world_seed != seed
                || impact_bodies.insert(body.id, body).is_some()
            {
                return Err(invalid("invalid checkpoint impact body"));
            }
        }
        let count = reader.u32()? as usize;
        if count > minimium_world::terrain_edits::MAX_TERRAIN_EDITS {
            return Err(invalid("too many checkpoint craters"));
        }
        let mut craters = BTreeMap::new();
        for _ in 0..count {
            let crater = reader.crater()?;
            if !crater.valid() || craters.insert(crater.id, crater).is_some() {
                return Err(invalid("invalid checkpoint crater"));
            }
        }
        if craters
            .keys()
            .any(|id| impact_bodies.get(id).is_none_or(|body| body.phase == 0))
        {
            return Err(invalid("checkpoint crater has no committed impact body"));
        }
        let water_catalog_digest = reader.digest()?;
        state.worlds.insert(
            id,
            RestoredWorld {
                world_seed: seed,
                last_mutation,
                chunks,
                fluids,
                impact_bodies,
                craters,
                water_catalog_digest,
            },
        );
    }
    let count = reader.u32()? as usize;
    if count > 1_000_000 {
        return Err(invalid("checkpoint inventory count exceeds limit"));
    }
    for _ in 0..count {
        let profile = PlayerProfileId::from_bytes(reader.fixed()?);
        if state
            .inventories
            .insert(profile, reader.inventory()?)
            .is_some()
        {
            return Err(invalid("duplicate checkpoint inventory"));
        }
    }
    reader.finish()?;
    Ok(state)
}
