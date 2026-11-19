//! Bounded durable impact log. Acknowledgement follows sync_all; replay is idempotent.
//! Periodic compaction retains every crater, including collected meteorites.
use minimium_world::{
    impact::{BodyState, BODY_RECORD_BYTES},
    world::WorldInstanceId,
};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
    time::Instant,
};

const MAGIC: &[u8; 8] = b"MIMPACT2";
mod memory;
pub(crate) use memory::MemoryImpacts;
pub const MAX_BODIES: usize = 4096;
const COMPACT_BYTES: u64 = 4 * 1024 * 1024;
// One parent plus one daughter is a single checksummed durable transaction.
const PAYLOAD_BYTES: usize = 1 + 2 * BODY_RECORD_BYTES;
const RECORD_BYTES: usize = PAYLOAD_BYTES + 8;
type Bodies = BTreeMap<(WorldInstanceId, u64), BodyState>;

enum Request {
    Commit(Vec<BodyState>),
    Stop,
}
pub struct ImpactJournal {
    requests: mpsc::SyncSender<Request>,
    completions: mpsc::Receiver<Result<Vec<BodyState>, io::Error>>,
    worker: Option<thread::JoinHandle<()>>,
}

/// Bounded durable impact checkpoint port. Queue acceptance is not a durable ACK.
pub trait ImpactJournalPort: Send {
    fn submit_batch(&self, batch: Vec<BodyState>) -> io::Result<()>;
    fn poll(&self) -> Option<io::Result<Vec<BodyState>>>;
    fn shutdown_until(&mut self, deadline: Instant) -> io::Result<()>;
}

impl ImpactJournalPort for ImpactJournal {
    fn submit_batch(&self, batch: Vec<BodyState>) -> io::Result<()> {
        ImpactJournal::submit_batch(self, batch)
    }
    fn poll(&self) -> Option<io::Result<Vec<BodyState>>> {
        ImpactJournal::poll(self)
    }
    fn shutdown_until(&mut self, deadline: Instant) -> io::Result<()> {
        ImpactJournal::shutdown_until(self, deadline)
    }
}

impl ImpactJournal {
    pub fn open(directory: &Path) -> io::Result<(Self, Vec<BodyState>)> {
        let path = directory.join("impacts.v2");
        let (file, bodies) = load(&path)?;
        let restored = bodies.values().copied().collect();
        let (requests, receiver) = mpsc::sync_channel(16);
        let (sender, completions) = mpsc::sync_channel(16);
        let worker = thread::Builder::new()
            .name("impact-persistence".into())
            .spawn(move || {
                let mut file = Some(file);
                let mut bodies = bodies;
                while let Ok(request) = receiver.recv() {
                    let Request::Commit(batch) = request else {
                        break;
                    };
                    let result = commit(&path, &mut file, &mut bodies, &batch).map(|()| batch);
                    let failed = result.is_err();
                    if sender.send(result).is_err() || failed {
                        break;
                    }
                }
            })?;
        Ok((
            Self {
                requests,
                completions,
                worker: Some(worker),
            },
            restored,
        ))
    }

    pub fn submit(&self, body: BodyState) -> io::Result<()> {
        self.submit_batch(vec![body])
    }

    pub fn submit_batch(&self, batch: Vec<BodyState>) -> io::Result<()> {
        if batch.is_empty() || batch.len() > 2 || batch.iter().any(|body| !body.valid()) {
            return Err(invalid("invalid body state"));
        }
        self.requests
            .try_send(Request::Commit(batch))
            .map_err(|e| io::Error::other(format!("impact journal unavailable: {e}")))
    }

    pub fn poll(&self) -> Option<io::Result<Vec<BodyState>>> {
        self.completions.try_recv().ok()
    }

    pub fn shutdown_until(&mut self, deadline: Instant) -> io::Result<()> {
        let mut stopped = false;
        let mut failure = None;
        while self.worker.as_ref().is_some_and(|w| !w.is_finished()) && Instant::now() < deadline {
            if !stopped {
                stopped = match self.requests.try_send(Request::Stop) {
                    Ok(()) | Err(mpsc::TrySendError::Disconnected(_)) => true,
                    Err(mpsc::TrySendError::Full(_)) => false,
                };
            }
            // Drain bounded completions so a finishing writer cannot wait on its receiver.
            while let Some(result) = self.poll() {
                if let Err(error) = result {
                    failure = Some(error);
                }
            }
            thread::sleep(std::time::Duration::from_millis(2));
        }
        while let Some(result) = self.poll() {
            if let Err(error) = result {
                failure = Some(error);
            }
        }
        match self.worker.take() {
            Some(w) if w.is_finished() => w
                .join()
                .map_err(|_| io::Error::other("impact writer panicked"))?,
            Some(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "impact writer shutdown",
                ))
            }
            None => {}
        }
        failure.map_or(Ok(()), Err)
    }
}

impl Drop for ImpactJournal {
    fn drop(&mut self) {
        let _ = self.requests.try_send(Request::Stop);
    }
}

fn load(path: &Path) -> io::Result<(File, Bodies)> {
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .append(true)
        .open(path)?;
    if file.metadata()?.len() == 0 {
        file.write_all(MAGIC)?;
        file.sync_all()?;
    }
    if file.metadata()?.len() > COMPACT_BYTES * 2 {
        return Err(invalid("impact log too large"));
    }
    let mut bytes = Vec::new();
    file.seek(SeekFrom::Start(0))?;
    file.read_to_end(&mut bytes)?;
    if !bytes.starts_with(MAGIC) {
        return Err(invalid("unsupported impact log"));
    }
    let mut bodies = Bodies::new();
    let complete = (bytes.len() - MAGIC.len()) / RECORD_BYTES;
    for record in bytes[MAGIC.len()..].chunks_exact(RECORD_BYTES) {
        let payload = &record[..PAYLOAD_BYTES];
        let expected = u64::from_le_bytes(record[PAYLOAD_BYTES..].try_into().unwrap());
        if checksum(payload) != expected {
            return Err(invalid("impact checksum mismatch"));
        }
        if !(1..=2).contains(&payload[0]) {
            return Err(invalid("invalid impact batch"));
        }
        for i in 0..usize::from(payload[0]) {
            let at = 1 + i * BODY_RECORD_BYTES;
            let body = BodyState::decode(&payload[at..at + BODY_RECORD_BYTES])
                .ok_or_else(|| invalid("invalid saved impact"))?;
            insert(&mut bodies, body)?;
        }
    }
    // Only an incomplete trailing write is discarded; damaged complete records fail closed.
    let valid_length = (MAGIC.len() + complete * RECORD_BYTES) as u64;
    if valid_length != bytes.len() as u64 {
        file.set_len(valid_length)?;
        file.sync_all()?;
    }
    Ok((file, bodies))
}

fn commit(
    path: &PathBuf,
    file: &mut Option<File>,
    bodies: &mut Bodies,
    batch: &[BodyState],
) -> io::Result<()> {
    let mut staged = bodies.clone();
    for &body in batch {
        insert(&mut staged, body)?;
    }
    let writer = file
        .as_mut()
        .ok_or_else(|| invalid("impact journal closed"))?;
    write_record(writer, batch)?;
    writer.sync_all()?;
    *bodies = staged;
    if writer.metadata()?.len() > COMPACT_BYTES {
        let temporary = path.with_extension("v2.tmp");
        drop(file.take());
        minimium_platform::files::atomic_write_with(&temporary, path, |out| {
            out.write_all(MAGIC)?;
            for saved in bodies.values() {
                write_record(out, &[*saved])?;
            }
            Ok(())
        })?;
        *file = Some(OpenOptions::new().read(true).append(true).open(path)?);
    }
    Ok(())
}

fn insert(bodies: &mut Bodies, body: BodyState) -> io::Result<()> {
    let key = (body.world, body.id);
    if bodies.get(&key).is_some_and(|old| !body.follows(old)) {
        return Err(invalid("impact state regressed or identity changed"));
    }
    if bodies.len() >= MAX_BODIES && !bodies.contains_key(&key) {
        return Err(invalid("impact capacity exceeded"));
    }
    bodies.insert(key, body);
    Ok(())
}

fn write_record(out: &mut impl Write, batch: &[BodyState]) -> io::Result<()> {
    let mut bytes = [0_u8; PAYLOAD_BYTES];
    bytes[0] = batch.len() as u8;
    for (i, body) in batch.iter().enumerate() {
        let at = 1 + i * BODY_RECORD_BYTES;
        bytes[at..at + BODY_RECORD_BYTES].copy_from_slice(&body.encode());
    }
    out.write_all(&bytes)?;
    out.write_all(&checksum(&bytes).to_le_bytes())
}

fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
