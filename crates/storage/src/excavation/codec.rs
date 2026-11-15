use super::*;
mod checkpoint;
pub(super) use checkpoint::{decode_checkpoint, encode_checkpoint};

pub(super) fn encode_mutation(m: &WorldMutation) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    put_u64(&mut out, m.id);
    put_u128(&mut out, m.world.get());
    put_u128(&mut out, m.world_seed);
    put_u16(&mut out, m.expected_chunk_revisions.len() as u16);
    for (key, revision) in &m.expected_chunk_revisions {
        for coordinate in key.coordinates {
            put_i32(&mut out, coordinate);
        }
        put_u64(&mut out, revision.0);
    }
    put_u16(&mut out, m.resulting_chunks.len() as u16);
    for chunk in &m.resulting_chunks {
        put_chunk(&mut out, chunk)?;
    }
    match m.inventory {
        None => out.push(0),
        Some((profile, inventory)) => {
            out.push(1);
            out.extend_from_slice(profile.as_bytes());
            put_inventory(&mut out, inventory);
        }
    }
    match &m.fluids {
        None => out.push(0),
        Some(fluid) => {
            out.push(1);
            put_fluid(&mut out, fluid)?;
        }
    }
    put_u16(&mut out, m.impact_bodies.len() as u16);
    for body in &m.impact_bodies {
        out.extend_from_slice(&body.encode());
    }
    put_u16(&mut out, m.craters.len() as u16);
    for crater in &m.craters {
        put_crater(&mut out, crater);
    }
    match m.water_catalog_digest {
        None => out.push(0),
        Some(digest) => {
            out.push(1);
            out.extend_from_slice(&digest);
        }
    }
    Ok(out)
}

fn put_chunk(out: &mut Vec<u8>, chunk: &ChunkImage) -> io::Result<()> {
    for coordinate in chunk.key().coordinates {
        put_i32(out, coordinate);
    }
    put_u64(out, chunk.revision().0);
    if chunk.samples().len() != SAMPLES_PER_CHUNK {
        return Err(invalid("invalid chunk sample count"));
    }
    for sample in chunk.samples() {
        out.extend_from_slice(&sample.distance_m.to_le_bytes());
        out.push(match sample.material {
            TerrainMaterial::Empty => 0,
            TerrainMaterial::Soil => 1,
            TerrainMaterial::Rock => 2,
        });
    }
    Ok(())
}

fn put_inventory(out: &mut Vec<u8>, inventory: Inventory) {
    put_u64(out, inventory.revision);
    out.push(inventory.equipped.map_or(0, |slot| slot + 1));
    for item in inventory.slots {
        out.push(item.map_or(0, |item| match item.kind {
            ToolKind::Shovel => 1,
            ToolKind::Pickaxe => 2,
        }));
    }
}

fn put_crater(out: &mut Vec<u8>, crater: &CraterStamp) {
    put_u64(out, crater.id);
    for value in crater.center {
        put_f64(out, value);
    }
    put_f64(out, crater.radius_m);
    put_f64(out, crater.depth_m);
    put_u64(out, crater.seed);
}

fn put_fluid(out: &mut Vec<u8>, fluid: &FluidSnapshot) -> io::Result<()> {
    put_u64(out, fluid.revision());
    put_u32(out, fluid.config().max_active_cells as u32);
    put_u32(out, fluid.cells().len() as u32);
    for (key, cell) in fluid.cells() {
        for coordinate in key {
            put_i32(out, *coordinate);
        }
        for value in [
            cell.capacity_m3,
            cell.volume_m3,
            cell.positive_velocity_m_s[0],
            cell.positive_velocity_m_s[1],
            cell.positive_velocity_m_s[2],
            cell.negative_boundary_velocity_m_s[0],
            cell.negative_boundary_velocity_m_s[1],
            cell.negative_boundary_velocity_m_s[2],
        ] {
            put_f64(out, value);
        }
    }
    put_u32(out, fluid.reservoirs().len() as u32);
    for (id, reservoir) in fluid.reservoirs() {
        put_u64(out, id.0);
        match &reservoir.kind {
            ReservoirKind::Ocean {
                level_m,
                exchanged_m3,
            } => {
                out.push(1);
                put_f64(out, *level_m);
                put_f64(out, *exchanged_m3);
                put_f64(out, reservoir.volume_correction_m3);
            }
            ReservoirKind::Finite { curve, volume_m3 } => {
                out.push(0);
                put_f64(out, *volume_m3);
                put_f64(out, reservoir.volume_correction_m3);
                put_u16(out, curve.len() as u16);
                for point in curve {
                    put_f64(out, point.level_m);
                    put_f64(out, point.volume_m3);
                }
            }
        }
    }
    Ok(())
}

pub(super) fn decode_mutation(bytes: &[u8]) -> io::Result<WorldMutation> {
    let mut reader = Decode { b: bytes, p: 0 };
    let id = reader.u64()?;
    let world = WorldInstanceId::new(reader.u128()?).ok_or_else(|| invalid("invalid world id"))?;
    let world_seed = reader.u128()?;
    let expected_count = reader.u16()? as usize;
    if expected_count > 64 {
        return Err(invalid("too many expected revisions"));
    }
    let mut expected_chunk_revisions = Vec::new();
    for _ in 0..expected_count {
        let coordinates = [reader.i32()?, reader.i32()?, reader.i32()?];
        expected_chunk_revisions.push((
            TerrainChunkKey { world, coordinates },
            TerrainRevision(reader.u64()?),
        ));
    }
    let chunk_count = reader.u16()? as usize;
    if chunk_count > 64 {
        return Err(invalid("invalid chunk count"));
    }
    let mut resulting_chunks = Vec::new();
    for _ in 0..chunk_count {
        resulting_chunks.push(Arc::new(reader.chunk(world)?));
    }
    let inventory = match reader.byte()? {
        0 => None,
        1 => {
            let profile = PlayerProfileId::from_bytes(reader.fixed()?);
            Some((profile, reader.inventory()?))
        }
        _ => return Err(invalid("invalid inventory marker")),
    };
    let fluids = match reader.byte()? {
        0 => None,
        1 => Some(reader.fluid(world)?),
        _ => return Err(invalid("invalid fluid marker")),
    };
    let body_count = reader.u16()? as usize;
    if body_count > 4096 {
        return Err(invalid("too many impact bodies"));
    }
    let mut impact_bodies = Vec::with_capacity(body_count);
    for _ in 0..body_count {
        let body = reader.body()?;
        if body.world != world {
            return Err(invalid("impact body world mismatch"));
        }
        impact_bodies.push(body);
    }
    let crater_count = reader.u16()? as usize;
    if crater_count > minimium_world::terrain_edits::MAX_TERRAIN_EDITS {
        return Err(invalid("too many crater stamps"));
    }
    let mut craters = Vec::with_capacity(crater_count);
    for _ in 0..crater_count {
        craters.push(reader.crater()?);
    }
    let water_catalog_digest = reader.digest()?;
    reader.finish()?;
    Ok(WorldMutation {
        id,
        world,
        world_seed,
        expected_chunk_revisions,
        resulting_chunks,
        inventory,
        fluids,
        impact_bodies,
        craters,
        water_catalog_digest,
    })
}

struct Decode<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Decode<'a> {
    fn take(&mut self, count: usize) -> io::Result<&'a [u8]> {
        let end = self
            .p
            .checked_add(count)
            .filter(|end| *end <= self.b.len())
            .ok_or_else(|| invalid("truncated excavation record"))?;
        let bytes = &self.b[self.p..end];
        self.p = end;
        Ok(bytes)
    }

    fn byte(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn fixed<const N: usize>(&mut self) -> io::Result<[u8; N]> {
        self.take(N)?
            .try_into()
            .map_err(|_| invalid("truncated field"))
    }

    fn u16(&mut self) -> io::Result<u16> {
        Ok(u16::from_le_bytes(self.fixed()?))
    }

    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.fixed()?))
    }

    fn i32(&mut self) -> io::Result<i32> {
        Ok(i32::from_le_bytes(self.fixed()?))
    }

    fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_le_bytes(self.fixed()?))
    }

    fn u128(&mut self) -> io::Result<u128> {
        Ok(u128::from_le_bytes(self.fixed()?))
    }

    fn f64(&mut self) -> io::Result<f64> {
        let value = f64::from_le_bytes(self.fixed()?);
        if value.is_finite() {
            Ok(value)
        } else {
            Err(invalid("invalid nonfinite value"))
        }
    }

    fn chunk(&mut self, world: WorldInstanceId) -> io::Result<ChunkImage> {
        let coordinates = [self.i32()?, self.i32()?, self.i32()?];
        let revision = TerrainRevision(self.u64()?);
        let mut samples = Vec::with_capacity(SAMPLES_PER_CHUNK);
        for _ in 0..SAMPLES_PER_CHUNK {
            let distance_m = self.f64()?;
            let material = match self.byte()? {
                0 => TerrainMaterial::Empty,
                1 => TerrainMaterial::Soil,
                2 => TerrainMaterial::Rock,
                _ => return Err(invalid("invalid terrain material")),
            };
            samples.push(TerrainSample {
                distance_m,
                material,
                revision,
            });
        }
        ChunkImage::from_samples(TerrainChunkKey { world, coordinates }, revision, samples)
            .ok_or_else(|| invalid("invalid chunk image"))
    }

    fn inventory(&mut self) -> io::Result<Inventory> {
        let revision = self.u64()?;
        let equipped = match self.byte()? {
            0 => None,
            1 => Some(0),
            2 => Some(1),
            _ => return Err(invalid("invalid equipped slot")),
        };
        let mut slots = [None; 2];
        for slot in &mut slots {
            *slot = match self.byte()? {
                0 => None,
                1 => Some(ItemRecord {
                    kind: ToolKind::Shovel,
                }),
                2 => Some(ItemRecord {
                    kind: ToolKind::Pickaxe,
                }),
                _ => return Err(invalid("invalid item kind")),
            };
        }
        let inventory = Inventory {
            slots,
            equipped,
            revision,
        };
        if equipped.is_some_and(|slot| inventory.slots[usize::from(slot)].is_none()) {
            return Err(invalid("equipped slot is empty"));
        }
        Ok(inventory)
    }

    fn fluid(&mut self, world: WorldInstanceId) -> io::Result<FluidSnapshot> {
        use minimium_world::fluid::WaterBodyId;

        let revision = self.u64()?;
        let max_active_cells = self.u32()? as usize;
        let cell_count = self.u32()? as usize;
        if cell_count > max_active_cells || cell_count > 524_288 {
            return Err(invalid("fluid cell count exceeds limit"));
        }
        let mut cells = BTreeMap::new();
        for _ in 0..cell_count {
            let key = [self.i32()?, self.i32()?, self.i32()?];
            let cell = CellWater {
                capacity_m3: self.f64()?,
                volume_m3: self.f64()?,
                positive_velocity_m_s: [self.f64()?, self.f64()?, self.f64()?],
                negative_boundary_velocity_m_s: [self.f64()?, self.f64()?, self.f64()?],
            };
            if !cell.valid() || cells.insert(key, cell).is_some() {
                return Err(invalid("invalid fluid cell"));
            }
        }
        let reservoir_count = self.u32()? as usize;
        if reservoir_count > 65_536 {
            return Err(invalid("too many fluid reservoirs"));
        }
        let mut reservoirs = BTreeMap::new();
        for _ in 0..reservoir_count {
            let id = WaterBodyId(self.u64()?);
            let (kind, volume_correction_m3) = match self.byte()? {
                1 => {
                    let level_m = self.f64()?;
                    let exchanged_m3 = self.f64()?;
                    let correction = self.f64()?;
                    (
                        ReservoirKind::Ocean {
                            level_m,
                            exchanged_m3,
                        },
                        correction,
                    )
                }
                0 => {
                    let volume_m3 = self.f64()?;
                    let correction = self.f64()?;
                    let point_count = self.u16()? as usize;
                    if !(2..=4096).contains(&point_count) {
                        return Err(invalid("invalid reservoir curve"));
                    }
                    let mut curve = Vec::with_capacity(point_count);
                    for _ in 0..point_count {
                        curve.push(VolumeLevel {
                            level_m: self.f64()?,
                            volume_m3: self.f64()?,
                        });
                    }
                    (ReservoirKind::Finite { curve, volume_m3 }, correction)
                }
                _ => return Err(invalid("invalid reservoir kind")),
            };
            let reservoir = Reservoir {
                id,
                kind,
                volume_correction_m3,
            };
            if !reservoir.valid() || reservoirs.insert(id, reservoir).is_some() {
                return Err(invalid("invalid reservoir"));
            }
        }
        FluidSnapshot::from_parts(
            world,
            FluidConfig { max_active_cells },
            revision,
            cells,
            reservoirs,
        )
        .map_err(|_| invalid("invalid fluid checkpoint"))
    }

    fn finish(&self) -> io::Result<()> {
        if self.p == self.b.len() {
            Ok(())
        } else {
            Err(invalid("trailing excavation bytes"))
        }
    }
}

impl Decode<'_> {
    fn body(&mut self) -> io::Result<BodyState> {
        BodyState::decode(&self.take(minimium_world::impact::BODY_RECORD_BYTES)?)
            .ok_or_else(|| invalid("invalid saved impact body"))
    }
    fn crater(&mut self) -> io::Result<CraterStamp> {
        let crater = CraterStamp {
            id: self.u64()?,
            center: [self.f64()?, self.f64()?, self.f64()?],
            radius_m: self.f64()?,
            depth_m: self.f64()?,
            seed: self.u64()?,
        };
        if crater.valid() {
            Ok(crater)
        } else {
            Err(invalid("invalid saved crater stamp"))
        }
    }
    fn digest(&mut self) -> io::Result<Option<[u8; 32]>> {
        match self.byte()? {
            0 => Ok(None),
            1 => Ok(Some(self.fixed()?)),
            _ => Err(invalid("invalid water catalog digest marker")),
        }
    }
}

fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes())
}
fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes())
}
fn put_i32(out: &mut Vec<u8>, value: i32) {
    out.extend_from_slice(&value.to_le_bytes())
}
fn put_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_le_bytes())
}
fn put_u128(out: &mut Vec<u8>, value: u128) {
    out.extend_from_slice(&value.to_le_bytes())
}
fn put_f64(out: &mut Vec<u8>, value: f64) {
    out.extend_from_slice(&value.to_le_bytes())
}
pub(super) fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}
pub(super) fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
