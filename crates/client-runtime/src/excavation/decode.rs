use super::*;
use minimium_world::terrain::{TerrainMaterial, TerrainRevision, TerrainSample};

pub(super) fn decode_chunk(
    world: minimium_world::world::WorldInstanceId,
    coordinates: [i32; 3],
    revision: u64,
    bytes: &[u8],
) -> Result<ChunkImage, &'static str> {
    if revision == 0 || bytes.len() != CHUNK_BYTES {
        return Err("invalid chunk image size");
    }
    let key = TerrainChunkKey { world, coordinates };
    let samples = bytes
        .chunks_exact(9)
        .map(|raw| {
            let distance_m = f64::from_le_bytes(raw[..8].try_into().map_err(|_| "invalid sample")?);
            let material = match raw[8] {
                0 => TerrainMaterial::Empty,
                1 => TerrainMaterial::Soil,
                2 => TerrainMaterial::Rock,
                _ => return Err("invalid terrain material"),
            };
            if !distance_m.is_finite() {
                return Err("invalid terrain distance");
            }
            Ok(TerrainSample {
                distance_m,
                material,
                revision: TerrainRevision(revision),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    ChunkImage::from_samples(key, TerrainRevision(revision), samples).ok_or("invalid chunk image")
}

pub(super) fn decode_fluid(
    world: minimium_world::world::WorldInstanceId,
    revision: u64,
    bytes: &[u8],
) -> Result<minimium_fluids::FluidSnapshot, &'static str> {
    use minimium_fluids::{
        CellWater, FluidConfig, FluidSnapshot, Reservoir, ReservoirKind, VolumeLevel,
    };
    use minimium_world::fluid::WaterBodyId;

    let mut reader = SliceReader { bytes, offset: 0 };
    let config = FluidConfig {
        max_active_cells: reader.u32()? as usize,
    };
    let count = reader.u32()? as usize;
    if count > config.max_active_cells || count > 524_288 {
        return Err("fluid cell count exceeds limit");
    }
    let mut cells = BTreeMap::new();
    for _ in 0..count {
        let key = [reader.i32()?, reader.i32()?, reader.i32()?];
        let cell = CellWater {
            capacity_m3: reader.f64()?,
            volume_m3: reader.f64()?,
            positive_velocity_m_s: [reader.f64()?, reader.f64()?, reader.f64()?],
            negative_boundary_velocity_m_s: [reader.f64()?, reader.f64()?, reader.f64()?],
        };
        if !cell.valid() || cells.insert(key, cell).is_some() {
            return Err("invalid fluid cell");
        }
    }
    let count = reader.u32()? as usize;
    if count > 65_536 {
        return Err("too many reservoirs");
    }
    let mut reservoirs = BTreeMap::new();
    for _ in 0..count {
        let id = WaterBodyId(reader.u64()?);
        let (kind, volume_correction_m3) = match reader.byte()? {
            0 => {
                let volume_m3 = reader.f64()?;
                let correction = reader.f64()?;
                let point_count = reader.u16()? as usize;
                if !(2..=4096).contains(&point_count) {
                    return Err("invalid reservoir curve");
                }
                let mut curve = Vec::with_capacity(point_count);
                for _ in 0..point_count {
                    curve.push(VolumeLevel {
                        level_m: reader.f64()?,
                        volume_m3: reader.f64()?,
                    });
                }
                (ReservoirKind::Finite { curve, volume_m3 }, correction)
            }
            1 => {
                let level_m = reader.f64()?;
                let exchanged_m3 = reader.f64()?;
                let correction = reader.f64()?;
                (
                    ReservoirKind::Ocean {
                        level_m,
                        exchanged_m3,
                    },
                    correction,
                )
            }
            _ => return Err("invalid reservoir kind"),
        };
        let reservoir = Reservoir {
            id,
            kind,
            volume_correction_m3,
        };
        if !reservoir.valid() || reservoirs.insert(id, reservoir).is_some() {
            return Err("invalid reservoir");
        }
    }
    reader.finish()?;
    FluidSnapshot::from_parts(world, config, revision, cells, reservoirs)
        .map_err(|_| "invalid fluid snapshot")
}

struct SliceReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl SliceReader<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], &'static str> {
        let end = self
            .offset
            .checked_add(N)
            .filter(|end| *end <= self.bytes.len())
            .ok_or("truncated fluid snapshot")?;
        let value = self.bytes[self.offset..end]
            .try_into()
            .map_err(|_| "truncated fluid value")?;
        self.offset = end;
        Ok(value)
    }

    fn byte(&mut self) -> Result<u8, &'static str> {
        Ok(self.take::<1>()?[0])
    }

    fn u16(&mut self) -> Result<u16, &'static str> {
        Ok(u16::from_le_bytes(self.take()?))
    }

    fn u32(&mut self) -> Result<u32, &'static str> {
        Ok(u32::from_le_bytes(self.take()?))
    }

    fn i32(&mut self) -> Result<i32, &'static str> {
        Ok(i32::from_le_bytes(self.take()?))
    }

    fn u64(&mut self) -> Result<u64, &'static str> {
        Ok(u64::from_le_bytes(self.take()?))
    }

    fn f64(&mut self) -> Result<f64, &'static str> {
        let value = f64::from_le_bytes(self.take()?);
        if value.is_finite() {
            Ok(value)
        } else {
            Err("nonfinite fluid value")
        }
    }

    fn finish(&self) -> Result<(), &'static str> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err("trailing fluid bytes")
        }
    }
}
