//! Context-bound, bounded client replica for durable excavation images.
use minimium_protocol::{ExcavationMessage, WorldContext};
use minimium_terrain::{ChunkImage, TerrainConfig, TerrainSnapshot, TerrainStore};
use minimium_world::{items::Inventory, terrain::TerrainChunkKey};
use std::{collections::BTreeMap, sync::Arc};
mod decode;
mod water;
use decode::{decode_chunk, decode_fluid};

const CHUNK_BYTES: usize = 32 * 32 * 32 * 9;
const MAX_STAGED_BYTES: usize = 32 * 1024 * 1024;

struct StagedChunk {
    revision: u64,
    bytes: Vec<u8>,
}
struct StagedFluid {
    sync_id: u64,
    revision: u64,
    region_min: [i32; 3],
    region_max: [i32; 3],
    total: usize,
    bytes: Vec<u8>,
}

struct Batch {
    sync_id: u64,
    revision: u64,
    expected: usize,
    expected_craters: usize,
    crater_count: usize,
    planet_radius_m: f64,
    full: bool,
    chunks: BTreeMap<[i32; 3], Arc<ChunkImage>>,
    staged_craters: TerrainSnapshot,
    fluids: Option<Arc<minimium_fluids::FluidSnapshot>>,
    fluid_region: Option<([i32; 3], [i32; 3])>,
    active: Option<([i32; 3], StagedChunk)>,
}

/// Client-owned publication state. Chunk batches become visible only at BatchEnd.
pub struct ExcavationReplica {
    context: Option<WorldContext>,
    terrain: Option<Arc<TerrainStore>>,
    inventory: Inventory,
    batch: Option<Batch>,
    ready: bool,
    needs_resync: bool,
    resync_requested: bool,
    last_result: Option<minimium_world::items::ActionResult>,
    fluids: Option<Arc<minimium_fluids::FluidSnapshot>>,
    staged_fluid: Option<StagedFluid>,
    fluid_region: Option<([i32; 3], [i32; 3])>,
    published_fluid_sync_id: Option<u64>,
    craters: Option<Arc<TerrainSnapshot>>,
}

impl Default for ExcavationReplica {
    fn default() -> Self {
        Self {
            context: None,
            terrain: None,
            inventory: Inventory::default(),
            batch: None,
            ready: false,
            needs_resync: false,
            resync_requested: false,
            last_result: None,
            fluids: None,
            staged_fluid: None,
            fluid_region: None,
            published_fluid_sync_id: None,
            craters: None,
        }
    }
}

impl ExcavationReplica {
    pub fn apply(
        &mut self,
        context: WorldContext,
        message: ExcavationMessage,
    ) -> Result<(), &'static str> {
        if self.context.is_some_and(|current| current != context) {
            self.context = Some(context);
            self.terrain = None;
            self.batch = None;
            self.ready = false;
            self.needs_resync = false;
            self.resync_requested = false;
            self.fluids = None;
            self.staged_fluid = None;
            self.fluid_region = None;
            self.published_fluid_sync_id = None;
            self.craters = None;
        } else {
            self.context = Some(context);
        }
        match message {
            ExcavationMessage::Inventory { inventory } => {
                if inventory
                    .equipped
                    .is_some_and(|slot| slot >= 2 || inventory.slots[usize::from(slot)].is_none())
                {
                    return Err("invalid inventory");
                }
                if inventory.revision >= self.inventory.revision {
                    self.inventory = inventory;
                }
            }
            ExcavationMessage::ActionResult { result } => self.last_result = Some(result),
            ExcavationMessage::BatchBegin {
                sync_id,
                base_revision,
                revision,
                chunks,
                craters,
                planet_radius_m,
                full,
            } => {
                if sync_id == 0
                    || chunks > 64
                    || craters as usize > minimium_world::terrain_edits::MAX_TERRAIN_EDITS
                    || self.batch.is_some()
                    || self.staged_fluid.is_some()
                    || !planet_radius_m.is_finite()
                    || !(1.0..=6_371_000.0).contains(&planet_radius_m)
                {
                    return Err("invalid batch begin");
                }
                if let Some(store) = &self.terrain {
                    if (!full
                        && (base_revision != store.last_mutation() || revision < base_revision))
                        || (full && revision < store.last_mutation())
                    {
                        self.ready = false;
                        self.needs_resync = true;
                        return Err("terrain revision gap; resync required");
                    }
                } else if !full {
                    self.ready = false;
                    self.needs_resync = true;
                    return Err("initial full image required");
                }
                if !full
                    && self
                        .craters
                        .as_ref()
                        .is_none_or(|saved| saved.radius_m() != planet_radius_m)
                {
                    self.needs_resync = true;
                    return Err("initial crater full image required");
                }
                let staged_craters = if full {
                    TerrainSnapshot::empty(planet_radius_m)
                } else {
                    self.craters.as_ref().map_or_else(
                        || TerrainSnapshot::empty(planet_radius_m),
                        |saved| (**saved).clone(),
                    )
                };
                self.batch = Some(Batch {
                    sync_id,
                    revision,
                    expected: usize::from(chunks),
                    expected_craters: usize::from(craters),
                    crater_count: 0,
                    planet_radius_m,
                    full,
                    chunks: BTreeMap::new(),
                    staged_craters,
                    fluids: None,
                    fluid_region: None,
                    active: None,
                });
            }
            ExcavationMessage::ChunkBegin {
                sync_id,
                coordinates,
                revision,
                total_bytes,
            } => {
                let batch = self.batch.as_mut().ok_or("chunk outside batch")?;
                if batch.sync_id != sync_id
                    || batch.active.is_some()
                    || total_bytes as usize != CHUNK_BYTES
                    || batch.chunks.contains_key(&coordinates)
                {
                    return Err("invalid chunk begin");
                }
                if coordinates.iter().any(|c| !(-625_000..625_000).contains(c)) {
                    return Err("invalid chunk coordinates");
                }
                let staged = batch
                    .chunks
                    .len()
                    .checked_mul(CHUNK_BYTES)
                    .ok_or("staging overflow")?;
                if staged
                    .checked_add(CHUNK_BYTES)
                    .is_none_or(|size| size > MAX_STAGED_BYTES)
                {
                    return Err("staging budget exceeded");
                }
                batch.active = Some((
                    coordinates,
                    StagedChunk {
                        revision,
                        bytes: Vec::with_capacity(CHUNK_BYTES),
                    },
                ));
            }
            ExcavationMessage::ChunkPart {
                sync_id,
                coordinates,
                revision,
                offset,
                bytes,
            } => {
                let batch = self.batch.as_mut().ok_or("chunk part outside batch")?;
                if batch.sync_id != sync_id {
                    return Err("wrong sync id");
                }
                let (key, chunk) = batch.active.as_mut().ok_or("chunk part outside image")?;
                if *key != coordinates
                    || chunk.revision != revision
                    || offset as usize != chunk.bytes.len()
                    || bytes.is_empty()
                    || chunk
                        .bytes
                        .len()
                        .checked_add(bytes.len())
                        .is_none_or(|size| size > CHUNK_BYTES)
                {
                    return Err("invalid chunk part");
                }
                chunk.bytes.extend_from_slice(&bytes);
            }
            ExcavationMessage::ChunkEnd {
                sync_id,
                coordinates,
                revision,
            } => {
                let batch = self.batch.as_mut().ok_or("chunk end outside batch")?;
                if batch.sync_id != sync_id {
                    return Err("wrong sync id");
                }
                let (key, chunk) = batch.active.take().ok_or("chunk end outside image")?;
                if key != coordinates
                    || chunk.revision != revision
                    || chunk.bytes.len() != CHUNK_BYTES
                    || revision > batch.revision
                {
                    return Err("incomplete chunk image");
                }
                let image = decode_chunk(context.instance(), coordinates, revision, &chunk.bytes)?;
                if !batch.full {
                    let key = TerrainChunkKey {
                        world: context.instance(),
                        coordinates,
                    };
                    if let Some(previous) = self.terrain.as_ref().and_then(|store| store.chunk(key))
                    {
                        if revision < previous.revision().0
                            || (revision == previous.revision().0 && image != **previous)
                        {
                            return Err("delta chunk revision regressed or conflicted");
                        }
                    }
                }
                batch.chunks.insert(coordinates, Arc::new(image));
            }
            ExcavationMessage::BatchEnd { sync_id, revision } => {
                let batch = self.batch.take().ok_or("batch end without begin")?;
                if batch.sync_id != sync_id
                    || batch.revision != revision
                    || batch.active.is_some()
                    || batch.chunks.len() != batch.expected
                    || batch.crater_count != batch.expected_craters
                {
                    return Err("incomplete chunk batch");
                }
                let config = self.terrain.as_ref().map_or(
                    TerrainConfig {
                        max_chunks: 4096,
                        max_edit_radius_m: 3.0,
                    },
                    |store| store.config(),
                );
                let mut images: BTreeMap<TerrainChunkKey, Arc<ChunkImage>> = if batch.full {
                    BTreeMap::new()
                } else {
                    self.terrain
                        .as_ref()
                        .map(|store| {
                            store
                                .chunks()
                                .map(|chunk| (chunk.key(), Arc::clone(chunk)))
                                .collect()
                        })
                        .unwrap_or_default()
                };
                for chunk in batch.chunks.into_values() {
                    images.insert(chunk.key(), chunk);
                }
                let store = TerrainStore::from_chunks(
                    context.instance(),
                    config,
                    revision,
                    images.into_values(),
                )
                .ok_or("invalid terrain snapshot")?;
                self.terrain = Some(Arc::new(store));
                self.craters = Some(Arc::new(batch.staged_craters));
                if let Some(fluids) = batch.fluids {
                    self.fluids = Some(fluids);
                    self.published_fluid_sync_id = Some(sync_id);
                }
                if batch.fluid_region.is_some() {
                    self.fluid_region = batch.fluid_region;
                }
                self.ready = true;
                self.needs_resync = false;
                self.resync_requested = false;
            }
            ExcavationMessage::FluidBegin {
                sync_id,
                revision,
                total_bytes,
                region_min,
                region_max,
            } => {
                if sync_id == 0
                    || self.staged_fluid.is_some()
                    || total_bytes == 0
                    || total_bytes as usize > minimium_protocol::MAX_FLUID_SNAPSHOT_BYTES
                    || (0..3).any(|axis| {
                        region_min[axis] >= region_max[axis]
                            || region_max[axis].abs_diff(region_min[axis]) > 64
                    })
                {
                    return Err("invalid fluid begin");
                }
                if self
                    .fluids
                    .as_ref()
                    .is_some_and(|fluid| revision < fluid.revision())
                {
                    return Err("stale fluid revision");
                }
                if self
                    .batch
                    .as_ref()
                    .is_some_and(|batch| batch.sync_id != sync_id)
                {
                    return Err("fluid sync id does not match terrain batch");
                }
                self.staged_fluid = Some(StagedFluid {
                    sync_id,
                    revision,
                    region_min,
                    region_max,
                    total: total_bytes as usize,
                    bytes: Vec::with_capacity(total_bytes as usize),
                });
            }
            ExcavationMessage::FluidPart {
                sync_id,
                revision,
                offset,
                bytes,
            } => {
                let stage = self
                    .staged_fluid
                    .as_mut()
                    .ok_or("fluid part without begin")?;
                if self
                    .batch
                    .as_ref()
                    .is_some_and(|batch| batch.sync_id != sync_id)
                {
                    return Err("fluid sync id does not match terrain batch");
                }
                if stage.sync_id != sync_id
                    || stage.revision != revision
                    || offset as usize != stage.bytes.len()
                    || bytes.is_empty()
                    || stage
                        .bytes
                        .len()
                        .checked_add(bytes.len())
                        .is_none_or(|size| size > stage.total)
                {
                    return Err("invalid fluid part");
                }
                stage.bytes.extend_from_slice(&bytes);
            }
            ExcavationMessage::FluidEnd { sync_id, revision } => {
                let stage = self.staged_fluid.take().ok_or("fluid end without begin")?;
                if stage.sync_id != sync_id
                    || stage.revision != revision
                    || stage.bytes.len() != stage.total
                {
                    return Err("incomplete fluid snapshot");
                }
                let snapshot = Arc::new(decode_fluid(context.instance(), revision, &stage.bytes)?);
                if let Some(batch) = self.batch.as_mut() {
                    if batch.sync_id != sync_id {
                        return Err("fluid sync id does not match terrain batch");
                    }
                    batch.fluids = Some(snapshot);
                    batch.fluid_region = Some((stage.region_min, stage.region_max));
                } else {
                    self.fluids = Some(snapshot);
                    self.fluid_region = Some((stage.region_min, stage.region_max));
                    self.published_fluid_sync_id = Some(sync_id);
                }
            }
            ExcavationMessage::Crater {
                sync_id,
                planet_radius_m,
                stamp,
            } => {
                let batch = self.batch.as_mut().ok_or("crater outside batch")?;
                if batch.sync_id != sync_id
                    || batch.planet_radius_m != planet_radius_m
                    || batch.crater_count >= batch.expected_craters
                {
                    return Err("invalid crater batch record");
                }
                if !batch
                    .staged_craters
                    .insert(stamp)
                    .map_err(|_| "invalid crater stamp")?
                {
                    return Err("duplicate crater stamp");
                }
                batch.crater_count += 1;
            }
        }
        Ok(())
    }

    pub fn terrain(&self) -> Option<Arc<TerrainStore>> {
        self.terrain.as_ref().map(Arc::clone)
    }
    pub fn inventory(&self) -> Inventory {
        self.inventory
    }
    pub fn ready(&self, context: WorldContext) -> bool {
        self.context == Some(context) && self.ready
    }
    pub fn needs_resync(&self) -> bool {
        self.needs_resync
    }
    pub fn reject_batch_message(&mut self) -> bool {
        self.batch = None;
        self.staged_fluid = None;
        self.ready = false;
        self.needs_resync = true;
        if self.resync_requested {
            return false;
        }
        self.resync_requested = true;
        true
    }
    pub fn take_result(&mut self) -> Option<minimium_world::items::ActionResult> {
        self.last_result.take()
    }
}
