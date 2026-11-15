//! Bounded lazy chunk-image replication for one peer's current sync.
use minimium_fluids::{FluidSnapshot, ReservoirKind};
use minimium_protocol::{ExcavationMessage, MAX_EXCAVATION_PART_BYTES};
use minimium_terrain::ChunkImage;
use minimium_world::terrain_edits::CraterStamp;
use std::{
    ops::Bound::{Excluded, Included, Unbounded},
    sync::Arc,
};

const SAMPLES_PER_CHUNK: usize = 32 * 32 * 32;

/// Produces one small protocol message at a time; it never queues encoded images.
pub struct ChunkReplicationCursor {
    sync_id: u64,
    base_revision: u64,
    revision: u64,
    full: bool,
    chunks: Vec<Arc<ChunkImage>>,
    craters: Vec<CraterStamp>,
    planet_radius_m: f64,
    phase: Phase,
    chunk_index: usize,
    crater_index: usize,
    part_sample: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    BatchBegin,
    ChunkBegin,
    ChunkParts,
    ChunkEnd,
    Craters,
    BatchEnd,
    Done,
}

/// Streams a coherent cells-plus-reservoirs snapshot without staging it in memory.
pub struct FluidReplicationCursor {
    sync_id: u64,
    snapshot: Arc<FluidSnapshot>,
    region_min: [i32; 3],
    region_max: [i32; 3],
    cells: Vec<[i32; 3]>,
    cell_index: usize,
    total: usize,
    phase: u8,
    last_reservoir: Option<minimium_world::fluid::WaterBodyId>,
    curve_index: usize,
    pending: Option<(Vec<u8>, usize)>,
    offset: usize,
    started: bool,
    done: bool,
}

impl FluidReplicationCursor {
    pub fn new(
        sync_id: u64,
        snapshot: Arc<FluidSnapshot>,
        region_min: [i32; 3],
        region_max: [i32; 3],
    ) -> Option<Self> {
        if sync_id == 0 {
            return None;
        }
        if (0..3).any(|axis| {
            region_min[axis] >= region_max[axis] || region_max[axis].abs_diff(region_min[axis]) > 64
        }) {
            return None;
        }
        let cells: Vec<_> = snapshot
            .cells()
            .range((
                Included([region_min[0], i32::MIN, i32::MIN]),
                Excluded([region_max[0], i32::MIN, i32::MIN]),
            ))
            .map(|(key, _)| key)
            .filter(|key| {
                (0..3).all(|axis| region_min[axis] <= key[axis] && key[axis] < region_max[axis])
            })
            .copied()
            .collect();
        let mut total = 8_usize
            .checked_add(cells.len().checked_mul(76)?)?
            .checked_add(4)?;
        for reservoir in snapshot.reservoirs().values() {
            let size = match &reservoir.kind {
                ReservoirKind::Ocean { .. } => 33,
                ReservoirKind::Finite { curve, .. } => {
                    27_usize.checked_add(curve.len().checked_mul(16)?)?
                }
            };
            total = total.checked_add(size)?;
        }
        if total > minimium_protocol::MAX_FLUID_SNAPSHOT_BYTES {
            return None;
        }
        Some(Self {
            sync_id,
            snapshot,
            region_min,
            region_max,
            cells,
            cell_index: 0,
            total,
            phase: 0,
            last_reservoir: None,
            curve_index: 0,
            pending: None,
            offset: 0,
            started: false,
            done: false,
        })
    }

    pub fn is_done(&self) -> bool {
        self.done
    }

    pub fn next(&mut self) -> Option<ExcavationMessage> {
        if self.done {
            return None;
        }
        if !self.started {
            self.started = true;
            return Some(ExcavationMessage::FluidBegin {
                sync_id: self.sync_id,
                revision: self.snapshot.revision(),
                total_bytes: self.total as u32,
                region_min: self.region_min,
                region_max: self.region_max,
            });
        }
        if self.offset == self.total {
            self.done = true;
            return Some(ExcavationMessage::FluidEnd {
                sync_id: self.sync_id,
                revision: self.snapshot.revision(),
            });
        }

        let limit = MAX_EXCAVATION_PART_BYTES.min(self.total - self.offset);
        let mut bytes = Vec::with_capacity(limit);
        while bytes.len() < limit {
            if self.pending.is_none() {
                self.pending = Some((self.next_segment()?, 0));
            }
            let (segment, at) = self.pending.as_mut()?;
            let count = (limit - bytes.len()).min(segment.len() - *at);
            bytes.extend_from_slice(&segment[*at..*at + count]);
            *at += count;
            if *at == segment.len() {
                self.pending = None;
            }
        }

        let offset = self.offset;
        self.offset += bytes.len();
        Some(ExcavationMessage::FluidPart {
            sync_id: self.sync_id,
            revision: self.snapshot.revision(),
            offset: offset as u32,
            bytes,
        })
    }

    fn next_segment(&mut self) -> Option<Vec<u8>> {
        loop {
            match self.phase {
                0 => {
                    self.phase = 1;
                    let mut bytes = Vec::with_capacity(8);
                    bytes.extend_from_slice(
                        &(self.snapshot.config().max_active_cells as u32).to_le_bytes(),
                    );
                    bytes.extend_from_slice(&(self.cells.len() as u32).to_le_bytes());
                    return Some(bytes);
                }
                1 => {
                    if let Some(key) = self.cells.get(self.cell_index) {
                        self.cell_index += 1;
                        let cell = self.snapshot.cells().get(key)?;
                        let mut bytes = Vec::with_capacity(76);
                        for coordinate in key {
                            bytes.extend_from_slice(&coordinate.to_le_bytes());
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
                            bytes.extend_from_slice(&value.to_le_bytes());
                        }
                        return Some(bytes);
                    }
                    self.phase = 2;
                }
                2 => {
                    self.phase = 3;
                    return Some(
                        (self.snapshot.reservoirs().len() as u32)
                            .to_le_bytes()
                            .to_vec(),
                    );
                }
                3 => {
                    let next = match self.last_reservoir {
                        Some(id) => self
                            .snapshot
                            .reservoirs()
                            .range((Excluded(id), Unbounded))
                            .next(),
                        None => self.snapshot.reservoirs().iter().next(),
                    };
                    if let Some((id, reservoir)) = next {
                        self.last_reservoir = Some(*id);
                        let mut bytes = Vec::with_capacity(27);
                        bytes.extend_from_slice(&id.0.to_le_bytes());
                        match &reservoir.kind {
                            ReservoirKind::Ocean {
                                level_m,
                                exchanged_m3,
                            } => {
                                bytes.push(1);
                                bytes.extend_from_slice(&level_m.to_le_bytes());
                                bytes.extend_from_slice(&exchanged_m3.to_le_bytes());
                                bytes.extend_from_slice(
                                    &reservoir.volume_correction_m3.to_le_bytes(),
                                );
                            }
                            ReservoirKind::Finite { curve, volume_m3 } => {
                                bytes.push(0);
                                bytes.extend_from_slice(&volume_m3.to_le_bytes());
                                bytes.extend_from_slice(
                                    &reservoir.volume_correction_m3.to_le_bytes(),
                                );
                                bytes.extend_from_slice(&(curve.len() as u16).to_le_bytes());
                                self.phase = 4;
                                self.curve_index = 0;
                            }
                        }
                        return Some(bytes);
                    }
                    self.phase = 5;
                }
                4 => {
                    let id = self.last_reservoir?;
                    let reservoir = self.snapshot.reservoirs().get(&id)?;
                    let ReservoirKind::Finite { curve, .. } = &reservoir.kind else {
                        return None;
                    };
                    if self.curve_index < curve.len() {
                        let point = curve[self.curve_index];
                        self.curve_index += 1;
                        let mut bytes = Vec::with_capacity(16);
                        bytes.extend_from_slice(&point.level_m.to_le_bytes());
                        bytes.extend_from_slice(&point.volume_m3.to_le_bytes());
                        return Some(bytes);
                    }
                    self.phase = 3;
                }
                5 => return None,
                _ => return None,
            }
        }
    }
}

impl ChunkReplicationCursor {
    pub fn new(
        sync_id: u64,
        base_revision: u64,
        revision: u64,
        full: bool,
        chunks: Vec<Arc<ChunkImage>>,
        planet_radius_m: f64,
        craters: Vec<CraterStamp>,
    ) -> Option<Self> {
        if sync_id == 0
            || chunks.len() > 64
            || craters.len() > minimium_world::terrain_edits::MAX_TERRAIN_EDITS
            || !planet_radius_m.is_finite()
            || !(1.0..=6_371_000.0).contains(&planet_radius_m)
            || (full && base_revision != 0)
            || revision < base_revision
        {
            return None;
        }
        Some(Self {
            sync_id,
            base_revision,
            revision,
            full,
            chunks,
            craters,
            planet_radius_m,
            phase: Phase::BatchBegin,
            chunk_index: 0,
            crater_index: 0,
            part_sample: 0,
        })
    }

    pub fn is_done(&self) -> bool {
        self.phase == Phase::Done
    }

    pub fn next(&mut self) -> Option<ExcavationMessage> {
        loop {
            match self.phase {
                Phase::BatchBegin => {
                    self.phase = if !self.chunks.is_empty() {
                        Phase::ChunkBegin
                    } else if !self.craters.is_empty() {
                        Phase::Craters
                    } else {
                        Phase::BatchEnd
                    };
                    return Some(ExcavationMessage::BatchBegin {
                        sync_id: self.sync_id,
                        base_revision: self.base_revision,
                        revision: self.revision,
                        chunks: self.chunks.len() as u16,
                        craters: self.craters.len() as u16,
                        planet_radius_m: self.planet_radius_m,
                        full: self.full,
                    });
                }
                Phase::ChunkBegin => {
                    let chunk = self.chunks.get(self.chunk_index)?;
                    if chunk.samples().len() != SAMPLES_PER_CHUNK {
                        return None;
                    }
                    self.part_sample = 0;
                    self.phase = Phase::ChunkParts;
                    return Some(ExcavationMessage::ChunkBegin {
                        sync_id: self.sync_id,
                        coordinates: chunk.key().coordinates,
                        revision: chunk.revision().0,
                        total_bytes: (SAMPLES_PER_CHUNK * 9) as u32,
                    });
                }
                Phase::ChunkParts => {
                    let chunk = self.chunks.get(self.chunk_index)?;
                    if self.part_sample >= SAMPLES_PER_CHUNK {
                        self.phase = Phase::ChunkEnd;
                        continue;
                    }
                    let sample_count =
                        (MAX_EXCAVATION_PART_BYTES / 9).min(SAMPLES_PER_CHUNK - self.part_sample);
                    let mut bytes = Vec::with_capacity(sample_count * 9);
                    for sample in
                        &chunk.samples()[self.part_sample..self.part_sample + sample_count]
                    {
                        bytes.extend_from_slice(&sample.distance_m.to_le_bytes());
                        bytes.push(match sample.material {
                            minimium_world::terrain::TerrainMaterial::Empty => 0,
                            minimium_world::terrain::TerrainMaterial::Soil => 1,
                            minimium_world::terrain::TerrainMaterial::Rock => 2,
                        });
                    }
                    let offset = (self.part_sample * 9) as u32;
                    self.part_sample += sample_count;
                    return Some(ExcavationMessage::ChunkPart {
                        sync_id: self.sync_id,
                        coordinates: chunk.key().coordinates,
                        revision: chunk.revision().0,
                        offset,
                        bytes,
                    });
                }
                Phase::ChunkEnd => {
                    let chunk = self.chunks.get(self.chunk_index)?;
                    let message = ExcavationMessage::ChunkEnd {
                        sync_id: self.sync_id,
                        coordinates: chunk.key().coordinates,
                        revision: chunk.revision().0,
                    };
                    self.chunk_index += 1;
                    self.phase = if self.chunk_index < self.chunks.len() {
                        Phase::ChunkBegin
                    } else if !self.craters.is_empty() {
                        Phase::Craters
                    } else {
                        Phase::BatchEnd
                    };
                    return Some(message);
                }
                Phase::Craters => {
                    let stamp = *self.craters.get(self.crater_index)?;
                    self.crater_index += 1;
                    if self.crater_index == self.craters.len() {
                        self.phase = Phase::BatchEnd;
                    }
                    return Some(ExcavationMessage::Crater {
                        sync_id: self.sync_id,
                        planet_radius_m: self.planet_radius_m,
                        stamp,
                    });
                }
                Phase::BatchEnd => {
                    self.phase = Phase::Done;
                    return Some(ExcavationMessage::BatchEnd {
                        sync_id: self.sync_id,
                        revision: self.revision,
                    });
                }
                Phase::Done => return None,
            }
        }
    }
}
