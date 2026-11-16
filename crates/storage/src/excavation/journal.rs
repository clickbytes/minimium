use super::*;
use minimium_gameplay::{WorldChangeAck, WorldChangeCommitment, WorldChangeFailure};

enum Request {
    Commit(WorldChangePlan),
    Stop,
}

pub struct ExcavationJournal {
    path: std::path::PathBuf,
    lock: Option<File>,
    requests: mpsc::SyncSender<Request>,
    completions: mpsc::Receiver<Result<WorldChangePlan, PersistenceError>>,
    worker: Option<thread::JoinHandle<()>>,
    restore: Arc<Mutex<ExcavationRestore>>,
    config: JournalConfig,
}

/// Durable mutation journal port. `submit` only confirms bounded queue acceptance;
/// publication requires the exact immutable plan returned by `poll_acknowledged`.
pub trait ExcavationJournalPort: Send {
    fn submit(&self, plan: WorldChangePlan) -> Result<(), PersistenceError>;
    fn validate(&self, plan: &WorldChangePlan) -> Result<(), PersistenceError>;
    fn poll_acknowledged(&self) -> Option<Result<WorldChangeAck, PersistenceError>>;
    fn counters(&self) -> (u64, u64);
    fn journal_bytes(&self) -> io::Result<u64>;
    fn shutdown_until(&mut self, deadline: Instant) -> io::Result<()>;
}

impl ExcavationJournalPort for ExcavationJournal {
    fn submit(&self, plan: WorldChangePlan) -> Result<(), PersistenceError> {
        ExcavationJournal::submit(self, plan)
    }
    fn validate(&self, plan: &WorldChangePlan) -> Result<(), PersistenceError> {
        ExcavationJournal::validate(self, plan)
    }
    fn poll_acknowledged(&self) -> Option<Result<WorldChangeAck, PersistenceError>> {
        ExcavationJournal::poll_acknowledged(self)
    }
    fn counters(&self) -> (u64, u64) {
        ExcavationJournal::counters(self)
    }
    fn journal_bytes(&self) -> io::Result<u64> {
        ExcavationJournal::journal_bytes(self)
    }
    fn shutdown_until(&mut self, deadline: Instant) -> io::Result<()> {
        ExcavationJournal::shutdown_until(self, deadline)
    }
}

#[derive(Debug)]
pub struct PersistenceError {
    pub kind: WorldChangeFailure,
    source: Option<io::Error>,
}

impl PersistenceError {
    pub fn new(kind: WorldChangeFailure) -> Self {
        Self { kind, source: None }
    }
    pub fn with_source(kind: WorldChangeFailure, source: io::Error) -> Self {
        Self {
            kind,
            source: Some(source),
        }
    }
    pub fn source_error(&self) -> Option<&io::Error> {
        self.source.as_ref()
    }
}
impl std::fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.source {
            Some(source) => write!(f, "world change persistence {:?}: {source}", self.kind),
            None => write!(f, "world change persistence {:?}", self.kind),
        }
    }
}
impl std::error::Error for PersistenceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|e| e as _)
    }
}
impl From<io::Error> for PersistenceError {
    fn from(source: io::Error) -> Self {
        Self::with_source(WorldChangeFailure::Failure, source)
    }
}

impl ExcavationJournal {
    pub fn preflight(path: &Path) -> io::Result<()> {
        preflight_header(path)
    }

    pub fn open(path: &Path) -> io::Result<(Self, WorldChangeState)> {
        Self::open_with_config(path, JournalConfig::default())
    }

    pub fn open_with_config(
        path: &Path,
        config: JournalConfig,
    ) -> io::Result<(Self, WorldChangeState)> {
        if !config.valid() {
            return Err(invalid("invalid excavation journal budget"));
        }
        preflight_header(path)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let lock_path = path.with_extension("excavation.lock");
        let lock = minimium_platform::files::try_lock_file(&lock_path)?.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::WouldBlock,
                "excavation journal is already open",
            )
        })?;
        let (file, restore) = load(path, config)?;
        let restored = restore.clone();
        let (requests, receiver) = mpsc::sync_channel(QUEUE_CAPACITY);
        let (sender, completions) = mpsc::sync_channel(QUEUE_CAPACITY);
        let worker_path = path.to_owned();
        let worker_restore = restore.clone();
        let worker_lock = lock.try_clone()?;
        let worker = thread::Builder::new()
            .name("excavation-journal".into())
            .spawn(move || {
                let _worker_lock = worker_lock;
                let mut file = Some(file);
                let mut state = worker_restore;
                while let Ok(request) = receiver.recv() {
                    let Request::Commit(mutation) = request else {
                        break;
                    };
                    let result = commit(&worker_path, &mut file, &mut state, mutation, config);
                    let fatal = result.as_ref().is_err_and(|error| {
                        matches!(
                            error.kind,
                            WorldChangeFailure::Failure | WorldChangeFailure::Unavailable
                        )
                    });
                    if sender.send(result).is_err() || fatal {
                        break;
                    }
                }
            })?;
        Ok((
            Self {
                path: path.to_owned(),
                lock: Some(lock),
                requests,
                completions,
                worker: Some(worker),
                restore: Arc::new(Mutex::new(restored.clone())),
                config,
            },
            restored,
        ))
    }

    /// Queue capacity is eight transactions; each encoded transaction is capped at 32 MiB.
    pub fn submit(&self, plan: WorldChangePlan) -> Result<(), PersistenceError> {
        self.validate(&plan)?;
        self.requests
            .try_send(Request::Commit(plan))
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => PersistenceError::new(WorldChangeFailure::Busy),
                mpsc::TrySendError::Disconnected(_) => {
                    PersistenceError::new(WorldChangeFailure::Unavailable)
                }
            })
    }

    /// Checks capacity and optimistic revisions without writing or queueing the mutation.
    pub fn validate(&self, plan: &WorldChangePlan) -> Result<(), PersistenceError> {
        let state = self
            .restore
            .lock()
            .map_err(|_| PersistenceError::new(WorldChangeFailure::Failure))?;
        let mutation = stored_mutation(plan);
        validate_mutation(&mutation, &state).map_err(|error| {
            PersistenceError::with_source(
                error.kind,
                io::Error::new(io::ErrorKind::InvalidData, error.detail),
            )
        })?;
        let size = encode_mutation(&mutation)
            .map_err(|e| PersistenceError::with_source(WorldChangeFailure::Failure, e))?
            .len();
        if size > self.config.transaction_budget {
            return Err(PersistenceError::new(WorldChangeFailure::LimitExceeded));
        }
        let checkpoint = checkpoint_size(&state, &mutation).map_err(PersistenceError::new)?;
        if checkpoint as u64 > self.config.byte_budget {
            return Err(PersistenceError::new(WorldChangeFailure::LimitExceeded));
        }
        Ok(())
    }

    pub fn poll_acknowledged(&self) -> Option<Result<WorldChangeAck, PersistenceError>> {
        let result = self.completions.try_recv().ok()?;
        if let Ok(plan) = &result {
            if let Ok(mut state) = self.restore.lock() {
                apply(&mut state, &stored_mutation(plan));
            }
        }
        Some(result.map(|plan| WorldChangeAck {
            plan,
            commitment: WorldChangeCommitment::Durable,
        }))
    }

    pub fn counters(&self) -> (u64, u64) {
        self.restore
            .lock()
            .map(|state| (state.mutation_count, state.last_mutation_id))
            .unwrap_or_default()
    }

    pub fn journal_bytes(&self) -> io::Result<u64> {
        Ok(std::fs::metadata(&self.path)?.len())
    }

    pub fn shutdown_until(&mut self, deadline: Instant) -> io::Result<()> {
        let mut stop_sent = false;
        let mut failure = None;
        while self.worker.as_ref().is_some_and(|w| !w.is_finished()) && Instant::now() < deadline {
            if !stop_sent {
                stop_sent = match self.requests.try_send(Request::Stop) {
                    Ok(()) | Err(mpsc::TrySendError::Disconnected(_)) => true,
                    Err(mpsc::TrySendError::Full(_)) => false,
                };
            }
            while let Some(result) = self.poll_acknowledged() {
                if let Err(error) = result {
                    failure = Some(error);
                }
            }
            thread::sleep(std::time::Duration::from_millis(2));
        }
        match self.worker.take() {
            Some(worker) if worker.is_finished() => worker
                .join()
                .map_err(|_| io::Error::other("excavation writer panicked"))?,
            Some(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "excavation journal shutdown timed out",
                ));
            }
            None => {}
        }
        while let Some(result) = self.poll_acknowledged() {
            if let Err(error) = result {
                failure = Some(error);
            }
        }
        if let Some(error) = failure {
            return Err(io::Error::other(error));
        }
        drop(self.lock.take());
        Ok(())
    }
}

fn preflight_header(path: &Path) -> io::Result<()> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if file.metadata()?.len() == 0 {
        return Ok(());
    }
    let mut magic = [0_u8; 8];
    file.read_exact(&mut magic)
        .map_err(|_| invalid("truncated excavation journal header"))?;
    if &magic != MAGIC {
        return Err(invalid("unsupported excavation journal version"));
    }
    Ok(())
}

impl Drop for ExcavationJournal {
    fn drop(&mut self) {
        let _ = self.requests.try_send(Request::Stop);
        if self
            .worker
            .as_ref()
            .is_some_and(|worker| worker.is_finished())
        {
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }
}
