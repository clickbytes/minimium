//! Durable, bounded write-ahead journal for committed excavation transactions.
use minimium_core::identity::PlayerProfileId;
use minimium_fluids::{
    CellWater, FluidConfig, FluidSnapshot, Reservoir, ReservoirKind, VolumeLevel,
};
use minimium_terrain::ChunkImage;
use minimium_world::{
    impact::BodyState,
    items::{Inventory, ItemRecord, ToolKind},
    terrain::{TerrainChunkKey, TerrainMaterial, TerrainRevision, TerrainSample},
    terrain_edits::CraterStamp,
    world::WorldInstanceId,
};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs::{File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
    sync::{mpsc, Arc, Mutex},
    thread,
    time::Instant,
};

const MAGIC: &[u8; 8] = b"MEXCAV11";
const CHECKPOINT: &[u8; 8] = b"MEXCHK11";
pub const JOURNAL_FILE_NAME: &str = "excavation.v1";
pub const MAX_MUTATION_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_JOURNAL_BYTES: u64 = 128 * 1024 * 1024;
const QUEUE_CAPACITY: usize = 8;
const SAMPLES_PER_CHUNK: usize = 32 * 32 * 32;
const CHECKPOINT_AT: u64 = 32 * 1024 * 1024;
const MAX_REPLAY_MUTATIONS: usize = 1_000_000;

#[derive(Clone, Copy, Debug)]
pub struct JournalConfig {
    pub byte_budget: u64,
    pub transaction_budget: usize,
}
impl Default for JournalConfig {
    fn default() -> Self {
        Self {
            byte_budget: MAX_JOURNAL_BYTES,
            transaction_budget: MAX_MUTATION_BYTES,
        }
    }
}
impl JournalConfig {
    fn valid(self) -> bool {
        (1024 * 1024..=MAX_JOURNAL_BYTES).contains(&self.byte_budget)
            && (1024..=MAX_MUTATION_BYTES).contains(&self.transaction_budget)
    }
}

#[derive(Clone)]
pub struct WorldMutation {
    pub id: u64,
    pub world: WorldInstanceId,
    pub world_seed: u128,
    pub expected_chunk_revisions: Vec<(TerrainChunkKey, TerrainRevision)>,
    pub resulting_chunks: Vec<Arc<ChunkImage>>,
    pub inventory: Option<(PlayerProfileId, Inventory)>,
    pub fluids: Option<FluidSnapshot>,
    pub impact_bodies: Vec<BodyState>,
    pub craters: Vec<CraterStamp>,
    pub water_catalog_digest: Option<[u8; 32]>,
}

#[derive(Clone, Default)]
pub struct RestoredWorld {
    pub world_seed: u128,
    pub last_mutation: u64,
    pub chunks: BTreeMap<[i32; 3], Arc<ChunkImage>>,
    pub fluids: Option<FluidSnapshot>,
    pub impact_bodies: BTreeMap<u64, BodyState>,
    pub craters: BTreeMap<u64, CraterStamp>,
    pub water_catalog_digest: Option<[u8; 32]>,
}

#[derive(Clone, Default)]
pub struct ExcavationRestore {
    pub worlds: BTreeMap<WorldInstanceId, RestoredWorld>,
    pub inventories: HashMap<PlayerProfileId, Inventory>,
    pub last_mutation_id: u64,
    pub mutation_count: u64,
}

mod journal;
pub use journal::ExcavationJournal;
pub use journal::ExcavationJournalPort;
mod budget;
use budget::checkpoint_size;
mod validation;
use validation::validate_mutation;
mod memory;
pub(crate) use memory::MemoryExcavation;
mod restore_validation;
pub use restore_validation::validate_restore;

fn commit(
    path: &Path,
    file: &mut Option<File>,
    state: &mut ExcavationRestore,
    mutation: WorldMutation,
    config: JournalConfig,
) -> io::Result<WorldMutation> {
    validate_mutation(&mutation, state).map_err(|error| invalid_request(&error.to_string()))?;
    let bytes = encode_mutation(&mutation)?;
    if checkpoint_size(state, &mutation).map_err(|error| invalid_request(&error.to_string()))?
        as u64
        > config.byte_budget
    {
        return Err(invalid_request(
            "restored state exceeds configured byte budget",
        ));
    }
    let writer = file
        .as_mut()
        .ok_or_else(|| invalid("excavation journal closed"))?;
    let record_size = bytes
        .len()
        .checked_add(12)
        .ok_or_else(|| invalid("record size overflow"))?;
    if bytes.len() > config.transaction_budget
        || writer
            .metadata()?
            .len()
            .checked_add(record_size as u64)
            .is_none_or(|length| length > config.byte_budget)
    {
        return Err(invalid_request("excavation journal budget exceeded"));
    }
    writer.write_all(&(bytes.len() as u32).to_le_bytes())?;
    writer.write_all(&bytes)?;
    writer.write_all(&checksum(&bytes).to_le_bytes())?;
    writer.sync_all()?;
    apply(state, &mutation);
    if file.as_ref().unwrap().metadata()?.len() >= CHECKPOINT_AT.min(config.byte_budget / 2) {
        let _ = compact(path, state, file, config);
    }
    Ok(mutation)
}

fn apply(state: &mut ExcavationRestore, mutation: &WorldMutation) {
    let world = state.worlds.entry(mutation.world).or_default();
    world.world_seed = mutation.world_seed;
    if !mutation.resulting_chunks.is_empty() || !mutation.craters.is_empty() {
        world.last_mutation = mutation.id;
    }
    for chunk in &mutation.resulting_chunks {
        world
            .chunks
            .insert(chunk.key().coordinates, Arc::clone(chunk));
    }
    if let Some((profile, inventory)) = mutation.inventory {
        state.inventories.insert(profile, inventory);
    }
    if let Some(fluid) = &mutation.fluids {
        world.fluids = Some(fluid.clone());
    }
    if mutation.water_catalog_digest.is_some() {
        world.water_catalog_digest = mutation.water_catalog_digest;
    }
    for body in &mutation.impact_bodies {
        world.impact_bodies.insert(body.id, *body);
    }
    for crater in &mutation.craters {
        world.craters.insert(crater.id, *crater);
    }
    state.last_mutation_id = mutation.id;
    state.mutation_count = state.mutation_count.saturating_add(1);
}

fn load(path: &Path, config: JournalConfig) -> io::Result<(File, ExcavationRestore)> {
    let mut file = OpenOptions::new()
        .create(true)
        .read(true)
        .append(true)
        .open(path)?;
    if file.metadata()?.len() == 0 {
        file.write_all(MAGIC)?;
        file.sync_all()?;
    }
    let length = file.metadata()?.len();
    if length > config.byte_budget {
        return Err(invalid("excavation journal exceeds configured byte budget"));
    }
    let mut bytes = Vec::with_capacity(length as usize);
    // Creating the header advances the shared read/write cursor. Replay must
    // start at byte zero for both newly created and already existing journals.
    file.seek(SeekFrom::Start(0))?;
    file.read_to_end(&mut bytes)?;
    if !bytes.starts_with(MAGIC) {
        return Err(invalid("unsupported excavation journal version"));
    }
    let mut state = ExcavationRestore::default();
    let mut offset = MAGIC.len();
    let mut valid = offset;
    let mut replayed = 0usize;
    while offset < bytes.len() {
        if bytes.len() - offset < 4 {
            break;
        }
        let length = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        let remaining = bytes.len() - offset;
        let Some(record_size) = length.checked_add(12) else {
            break;
        };
        if record_size > remaining {
            break;
        }
        if length > MAX_JOURNAL_BYTES as usize {
            return Err(invalid("oversized excavation record"));
        }
        let end = offset
            .checked_add(record_size)
            .ok_or_else(|| invalid("journal offset overflow"))?;
        let record = &bytes[offset + 4..offset + 4 + length];
        let expected = u64::from_le_bytes(bytes[offset + 4 + length..end].try_into().unwrap());
        if checksum(record) != expected {
            return Err(invalid("excavation checksum mismatch"));
        }
        if record.starts_with(CHECKPOINT) {
            state = decode_checkpoint(&record[CHECKPOINT.len()..])?;
            replayed = 0;
        } else {
            if record.len() > MAX_MUTATION_BYTES {
                return Err(invalid("oversized excavation mutation"));
            }
            let mutation = decode_mutation(record)?;
            validate_mutation(&mutation, &state)?;
            apply(&mut state, &mutation);
            replayed += 1;
        }
        if replayed > MAX_REPLAY_MUTATIONS {
            return Err(invalid("too many excavation records"));
        }
        offset = end;
        valid = end;
    }
    if valid != bytes.len() {
        file.set_len(valid as u64)?;
        file.sync_all()?;
    }
    Ok((file, state))
}

fn compact(
    path: &Path,
    state: &ExcavationRestore,
    file: &mut Option<File>,
    config: JournalConfig,
) -> io::Result<()> {
    // Atomic checkpoint stores the canonical latest committed state as one bounded snapshot.
    let bytes = encode_checkpoint(state)?;
    if bytes
        .len()
        .checked_add(20)
        .is_none_or(|size| size as u64 > config.byte_budget)
    {
        return Err(invalid("checkpoint exceeds configured byte limit"));
    }
    let temporary = path.with_extension("excavation.tmp");
    drop(file.take());
    let result = minimium_platform::files::atomic_write_with(&temporary, path, |out| {
        out.write_all(MAGIC)?;
        out.write_all(&(bytes.len() as u32).to_le_bytes())?;
        out.write_all(&bytes)?;
        out.write_all(&checksum(&bytes).to_le_bytes())
    });
    *file = Some(OpenOptions::new().read(true).append(true).open(path)?);
    result?;
    Ok(())
}

mod codec;
use codec::{
    checksum, decode_checkpoint, decode_mutation, encode_checkpoint, encode_mutation, invalid,
};

fn invalid_request(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
