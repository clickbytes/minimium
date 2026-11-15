use super::*;

enum Request {
    Commit(WorldMutation),
    Stop,
}

pub struct ExcavationJournal {
    path: std::path::PathBuf,
    lock: Option<File>,
    requests: mpsc::SyncSender<Request>,
    completions: mpsc::Receiver<io::Result<WorldMutation>>,
    worker: Option<thread::JoinHandle<()>>,
    restore: Arc<Mutex<ExcavationRestore>>,
    config: JournalConfig,
}

/// Durable mutation journal port. `submit` only confirms bounded queue acceptance;
/// publication requires the exact `WorldMutation` returned by `poll_durable`.
pub trait ExcavationJournalPort: Send {
    fn submit(&self, mutation: WorldMutation) -> io::Result<()>;
    fn validate(&self, mutation: &WorldMutation) -> io::Result<()>;
    fn poll_durable(&self) -> Option<io::Result<WorldMutation>>;
    fn counters(&self) -> (u64, u64);
    fn journal_bytes(&self) -> io::Result<u64>;
    fn shutdown_until(&mut self, deadline: Instant) -> io::Result<()>;
}

impl ExcavationJournalPort for ExcavationJournal {
    fn submit(&self, mutation: WorldMutation) -> io::Result<()> {
        ExcavationJournal::submit(self, mutation)
    }
    fn validate(&self, mutation: &WorldMutation) -> io::Result<()> {
        ExcavationJournal::validate(self, mutation)
    }
    fn poll_durable(&self) -> Option<io::Result<WorldMutation>> {
        ExcavationJournal::poll_durable(self)
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

impl ExcavationJournal {
    pub fn preflight(path: &Path) -> io::Result<()> {
        preflight_header(path)
    }

    pub fn open(path: &Path) -> io::Result<(Self, ExcavationRestore)> {
        Self::open_with_config(path, JournalConfig::default())
    }

    pub fn open_with_config(
        path: &Path,
        config: JournalConfig,
    ) -> io::Result<(Self, ExcavationRestore)> {
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
                    let fatal = result
                        .as_ref()
                        .is_err_and(|error| error.kind() != io::ErrorKind::InvalidInput);
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
    pub fn submit(&self, mutation: WorldMutation) -> io::Result<()> {
        self.validate(&mutation)?;
        self.requests
            .try_send(Request::Commit(mutation))
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "excavation journal queue is full",
                ),
                mpsc::TrySendError::Disconnected(_) => {
                    io::Error::new(io::ErrorKind::BrokenPipe, "excavation journal stopped")
                }
            })
    }

    /// Checks capacity and optimistic revisions without writing or queueing the mutation.
    pub fn validate(&self, mutation: &WorldMutation) -> io::Result<()> {
        let state = self
            .restore
            .lock()
            .map_err(|_| io::Error::other("excavation state poisoned"))?;
        validate_mutation(mutation, &state).map_err(|error| invalid_request(&error.to_string()))?;
        let size = encode_mutation(mutation)?.len();
        if size > self.config.transaction_budget {
            return Err(invalid_request(
                "excavation transaction exceeds configured budget",
            ));
        }
        let checkpoint = checkpoint_size(&state, mutation)
            .map_err(|error| invalid_request(&error.to_string()))?;
        if checkpoint as u64 > self.config.byte_budget {
            return Err(invalid_request(
                "restored state exceeds configured byte budget",
            ));
        }
        Ok(())
    }

    pub fn poll_durable(&self) -> Option<io::Result<WorldMutation>> {
        let result = self.completions.try_recv().ok()?;
        if let Ok(mutation) = &result {
            if let Ok(mut state) = self.restore.lock() {
                apply(&mut state, mutation);
            }
        }
        Some(result)
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
            while let Some(result) = self.poll_durable() {
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
        while let Some(result) = self.poll_durable() {
            if let Err(error) = result {
                failure = Some(error);
            }
        }
        if let Some(error) = failure {
            return Err(error);
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
