use super::*;
use std::time::Instant;

pub(super) struct MemorySnapshots {
    lease: Arc<Lease>,
    maximum: usize,
    pending: Option<Vec<u8>>,
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
            self.status.pending_saves = 0;
            let mut state = self
                .lease
                .state
                .lock()
                .map_err(|_| io::Error::other("memory backend poisoned"))?;
            state.snapshot = Some(bytes);
            self.status.last_save = SaveStatus::Complete;
        }
        Ok(())
    }
}
impl PersistencePort for MemorySnapshots {
    fn queue_save(&mut self, snapshot: SaveSnapshot) -> Result<(), StorageError> {
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
            self.pending = Some(bytes);
            self.status.pending_saves = 1;
            self.status.last_save = SaveStatus::Pending;
            Ok(())
        })();
        if let Err(error) = &result {
            self.status.last_save = SaveStatus::Failed(compact_error(error));
        }
        result
    }
    fn poll_completions(&mut self) {
        if let Err(error) = self.commit_pending() {
            self.status.last_save = SaveStatus::Failed(compact_error(&error));
        }
    }
    fn has_pending_save(&self) -> bool {
        self.pending.is_some()
    }
    fn reject_pending_save(&mut self) -> StorageError {
        let error = StorageError::busy("a world save is already pending");
        self.status.last_save = SaveStatus::Failed(compact_error(&error));
        error
    }
    fn status(&self) -> PersistenceStatus {
        self.status.clone()
    }
    fn shutdown_until(
        &mut self,
        snapshot: Option<SaveSnapshot>,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        if Instant::now() >= deadline {
            return Err(
                io::Error::new(io::ErrorKind::TimedOut, "memory save shutdown timeout").into(),
            );
        }
        self.commit_pending()?;
        if let Some(snapshot) = snapshot {
            self.queue_save(snapshot)?;
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
