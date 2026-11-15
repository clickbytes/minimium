use super::{
    player_store::{PlayerStore, SaveSnapshot},
    StorageError, WorldSaveInput,
};
use minimium_diagnostics::DomainLogger;
use minimium_environment::time::SavedClockState;
use minimium_telemetry::allocations::{self, Phase};
use std::{
    io,
    path::PathBuf,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

mod status;

use status::SaveBookkeeping;

enum PersistenceRequest {
    Save(SaveSnapshot),
    Shutdown(
        Option<SaveSnapshot>,
        mpsc::SyncSender<Result<(), StorageError>>,
    ),
    Stop,
}
pub struct PersistenceWorker {
    sender: mpsc::SyncSender<PersistenceRequest>,
    completions: mpsc::Receiver<Result<(), StorageError>>,
    thread: Option<thread::JoinHandle<()>>,
    status: SaveBookkeeping,
    last_queued_revision: u64,
}

impl PersistenceWorker {
    pub fn spawn(data_dir: PathBuf, logger: DomainLogger) -> Result<Self, StorageError> {
        let (sender, requests) = mpsc::sync_channel::<PersistenceRequest>(1);
        let (completion_sender, completions) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("minimium-storage".into())
            .spawn(move || {
                let _storage_phase = allocations::enter(Phase::Storage);
                let mut last_written_revision = 0;
                while let Ok(request) = requests.recv() {
                    match request {
                        PersistenceRequest::Save(snapshot) => {
                            let result = write_revisioned_snapshot(
                                snapshot,
                                &data_dir,
                                &mut last_written_revision,
                            );
                            let _ = completion_sender.send(result);
                        }
                        PersistenceRequest::Shutdown(snapshot, reply) => {
                            let result = snapshot.map_or(Ok(()), |snapshot| {
                                write_revisioned_snapshot(
                                    snapshot,
                                    &data_dir,
                                    &mut last_written_revision,
                                )
                            });
                            let _ = reply.send(result);
                            break;
                        }
                        PersistenceRequest::Stop => break,
                    }
                }
            })
            .map_err(StorageError::from_io)?;
        Ok(Self {
            sender,
            completions,
            thread: Some(worker),
            status: SaveBookkeeping::new(logger),
            last_queued_revision: 0,
        })
    }

    pub fn queue_save(&mut self, snapshot: SaveSnapshot) -> Result<(), StorageError> {
        if snapshot.revision() <= self.last_queued_revision {
            let error = StorageError::from(io::Error::new(
                io::ErrorKind::InvalidInput,
                "snapshot revision is not newer than the queued save",
            ));
            self.status.record_enqueue_failure(&error);
            return Err(error);
        }
        let revision = snapshot.revision();
        match self.sender.try_send(PersistenceRequest::Save(snapshot)) {
            Ok(()) => {
                self.last_queued_revision = revision;
                self.status.record_queued();
                Ok(())
            }
            Err(mpsc::TrySendError::Full(_)) => {
                let error = StorageError::busy("a world save is already queued");
                self.status.record_enqueue_failure(&error);
                Err(error)
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                let error = StorageError::worker_stopped("world storage worker stopped");
                self.status.record_enqueue_failure(&error);
                Err(error)
            }
        }
    }

    pub fn queue_world_save(
        &mut self,
        player_store: &mut PlayerStore,
        input: WorldSaveInput,
        saved_clock: SavedClockState,
    ) -> Result<(), StorageError> {
        if self.status.has_pending_save() {
            let error = StorageError::busy("a world save is already pending");
            self.status.record_enqueue_failure(&error);
            return Err(error);
        }
        let snapshot = player_store.save_snapshot(input, saved_clock)?;
        self.queue_save(snapshot)
    }

    pub fn poll_completions(&mut self) {
        loop {
            match self.completions.try_recv() {
                Ok(result) => {
                    self.status.record_completion(result);
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.status.record_worker_stopped();
                    break;
                }
            }
        }
    }

    pub fn has_pending_save(&self) -> bool {
        self.status.has_pending_save()
    }

    pub fn summary(&self) -> String {
        self.status.summary()
    }

    pub fn shutdown_until(
        &mut self,
        snapshot: Option<SaveSnapshot>,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        let stale_final = snapshot
            .as_ref()
            .is_some_and(|snapshot| snapshot.revision() <= self.last_queued_revision);
        let final_revision = snapshot.as_ref().map(SaveSnapshot::revision);
        let snapshot = if stale_final { None } else { snapshot };
        let (reply_sender, reply_receiver) = mpsc::sync_channel(1);
        let send_result = self.send_shutdown_until(snapshot, reply_sender, deadline);
        let save_result = match send_result {
            Ok(()) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                match reply_receiver.recv_timeout(remaining) {
                    Ok(result) => result,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        Err(StorageError::from(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "final save exceeded shutdown deadline",
                        )))
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => Err(StorageError::worker_stopped(
                        "storage worker stopped before shutdown completed",
                    )),
                }
            }
            Err(error) => Err(error),
        };
        let join_result = self.join_worker_until(deadline);
        self.poll_completions();
        if let Err(error) = &join_result {
            self.status.emit(
                "storage.worker_join_failed",
                minimium_diagnostics::Level::Error,
                format!("{}: {}", error.code(), crate::compact_error(error)),
            );
        }
        if stale_final {
            join_result?;
            return Err(StorageError::from(io::Error::new(
                io::ErrorKind::InvalidInput,
                "final snapshot revision is not newer than queued saves",
            )));
        }
        if save_result.is_ok() {
            if let Some(revision) = final_revision {
                self.last_queued_revision = self.last_queued_revision.max(revision);
            }
        }
        save_result?;
        join_result
    }

    fn send_shutdown_until(
        &self,
        snapshot: Option<SaveSnapshot>,
        reply: mpsc::SyncSender<Result<(), StorageError>>,
        deadline: Instant,
    ) -> Result<(), StorageError> {
        let mut request = PersistenceRequest::Shutdown(snapshot, reply);
        loop {
            match self.sender.try_send(request) {
                Ok(()) => return Ok(()),
                Err(mpsc::TrySendError::Disconnected(_)) => {
                    return Err(StorageError::worker_stopped("world storage worker stopped"));
                }
                Err(mpsc::TrySendError::Full(returned)) => request = returned,
            }
            if Instant::now() >= deadline {
                return Err(StorageError::from(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "storage shutdown queue remained full",
                )));
            }
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn join_worker_until(&mut self, deadline: Instant) -> Result<(), StorageError> {
        while self
            .thread
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
            && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(5));
        }
        if self
            .thread
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
        {
            return Err(StorageError::from(io::Error::new(
                io::ErrorKind::TimedOut,
                "storage worker exceeded shutdown deadline",
            )));
        }
        if let Some(worker) = self.thread.take() {
            worker
                .join()
                .map_err(|_| StorageError::worker_stopped("world storage worker panicked"))?;
        }
        Ok(())
    }
}

fn write_revisioned_snapshot(
    snapshot: SaveSnapshot,
    data_dir: &std::path::Path,
    last_written_revision: &mut u64,
) -> Result<(), StorageError> {
    if snapshot.revision() <= *last_written_revision {
        return Err(StorageError::from(io::Error::new(
            io::ErrorKind::InvalidInput,
            "stale snapshot revision cannot replace a newer save",
        )));
    }
    let revision = snapshot.revision();
    snapshot.write(data_dir)?;
    *last_written_revision = revision;
    Ok(())
}

impl Drop for PersistenceWorker {
    fn drop(&mut self) {
        if self.thread.is_some() {
            let _ = self.sender.try_send(PersistenceRequest::Stop);
            self.thread.take();
        }
    }
}
