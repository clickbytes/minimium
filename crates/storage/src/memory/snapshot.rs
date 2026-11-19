use super::*;
use std::time::Instant;

pub(super) struct MemorySnapshots {
    lease: Arc<Lease>,
    maximum: usize,
    pending: Option<Vec<u8>>,
    pending_job: Option<(SaveJobId, u64)>,
    completed: Vec<SaveCompletion>,
    last_revision: u64,
    status: PersistenceStatus,
    stopped: bool,
}
impl MemorySnapshots {
    pub(super) fn new(lease: Arc<Lease>, maximum: usize) -> Self {
        Self {
            lease,
            maximum,
            pending: None,
            pending_job: None,
            completed: Vec::new(),
            last_revision: 0,
            stopped: false,
            status: PersistenceStatus {
                pending_saves: 0,
                last_save: SaveStatus::Never,
            },
        }
    }
    fn commit_pending(&mut self) -> Result<(), StorageError> {
        if let Some(bytes) = self.pending.take() {
            let mut state = self
                .lease
                .state
                .lock()
                .map_err(|_| io::Error::other("memory backend poisoned"))?;
            state.snapshot = Some(bytes);
            self.status.pending_saves = 0;
            self.status.last_save = SaveStatus::Complete;
            if let Some((id, revision)) = self.pending_job.take() {
                self.completed.push(SaveCompletion {
                    id,
                    revision,
                    durability: SaveDurability::Volatile,
                    result: Ok(()),
                });
            }
        }
        Ok(())
    }
}
impl PersistencePort for MemorySnapshots {
    fn queue_save(&mut self, id: SaveJobId, snapshot: SaveSnapshot) -> Result<(), StorageError> {
        let result = (|| {
            if self.stopped {
                return Err(StorageError::worker_stopped("memory snapshots stopped"));
            }
            if self.pending.is_some() {
                return Err(StorageError::busy("a world save is already pending"));
            }
            if snapshot.revision() <= self.last_revision {
                return Err(
                    io::Error::new(io::ErrorKind::InvalidData, "stale snapshot revision").into(),
                );
            }
            let bytes = crate::format::encode_bytes(&snapshot, self.maximum)?;
            self.last_revision = snapshot.revision();
            self.pending_job = Some((id, snapshot.revision()));
            self.pending = Some(bytes);
            self.status.pending_saves = 1;
            self.status.last_save = SaveStatus::Pending;
            Ok(())
        })();
        if let Err(error) = &result {
            if self.pending.is_none() {
                self.status.last_save = SaveStatus::Failed(compact_error(error));
            }
        }
        result
    }
    fn poll_completions(&mut self) -> Vec<SaveCompletion> {
        if let Err(error) = self.commit_pending() {
            self.status.pending_saves = 0;
            self.status.last_save = SaveStatus::Failed(compact_error(&error));
            if let Some((id, revision)) = self.pending_job.take() {
                self.completed.push(SaveCompletion {
                    id,
                    revision,
                    durability: SaveDurability::Volatile,
                    result: Err(error),
                });
            }
        }
        std::mem::take(&mut self.completed)
    }
    fn has_pending_save(&self) -> bool {
        self.pending.is_some()
    }
    fn reject_pending_save(&mut self) -> StorageError {
        let error = StorageError::busy("a world save is already pending");
        error
    }
    fn status(&self) -> PersistenceStatus {
        self.status.clone()
    }
    fn completion_durability(&self) -> SaveDurability {
        SaveDurability::Volatile
    }
    fn shutdown_until(
        &mut self,
        snapshot: Option<(SaveJobId, SaveSnapshot)>,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        if Instant::now() >= deadline {
            return Err(
                io::Error::new(io::ErrorKind::TimedOut, "memory save shutdown timeout").into(),
            );
        }
        self.commit_pending()?;
        if let Some((id, snapshot)) = snapshot {
            self.queue_save(id, snapshot)?;
            self.commit_pending()?;
        }
        self.stopped = true;
        if Instant::now() >= deadline {
            return Err(
                io::Error::new(io::ErrorKind::TimedOut, "memory save shutdown timeout").into(),
            );
        }
        Ok(())
    }
}
