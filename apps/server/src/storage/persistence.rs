use super::{
    compact_error,
    player_store::{PlayerStore, SaveSnapshot},
};
use minimium_core::world::World;
use minimium_telemetry::allocations::{self, Phase};
use std::{io, path::PathBuf, sync::mpsc, thread};

enum PersistenceRequest {
    Save(SaveSnapshot),
    Shutdown(Option<SaveSnapshot>, mpsc::SyncSender<io::Result<()>>),
    Stop,
}

enum SaveStatus {
    Never,
    Pending,
    Complete,
    Failed(String),
}

impl std::fmt::Display for SaveStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Never => f.write_str("never"),
            Self::Pending => f.write_str("pending"),
            Self::Complete => f.write_str("ok"),
            Self::Failed(message) => write!(f, "error_{message}"),
        }
    }
}

pub(crate) struct PersistenceWorker {
    sender: mpsc::SyncSender<PersistenceRequest>,
    completions: mpsc::Receiver<io::Result<()>>,
    thread: Option<thread::JoinHandle<()>>,
    pending_saves: usize,
    last_save: SaveStatus,
}

impl PersistenceWorker {
    pub(crate) fn spawn(data_dir: PathBuf) -> io::Result<Self> {
        let (sender, requests) = mpsc::sync_channel::<PersistenceRequest>(1);
        let (completion_sender, completions) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("minimium-storage".into())
            .spawn(move || {
                let _storage_phase = allocations::enter(Phase::Storage);
                while let Ok(request) = requests.recv() {
                    match request {
                        PersistenceRequest::Save(snapshot) => {
                            let _ = completion_sender.send(snapshot.write(&data_dir));
                        }
                        PersistenceRequest::Shutdown(snapshot, reply) => {
                            let result =
                                snapshot.map_or(Ok(()), |snapshot| snapshot.write(&data_dir));
                            let _ = reply.send(result);
                            break;
                        }
                        PersistenceRequest::Stop => break,
                    }
                }
            })?;
        Ok(Self {
            sender,
            completions,
            thread: Some(worker),
            pending_saves: 0,
            last_save: SaveStatus::Never,
        })
    }

    pub(crate) fn queue_save(&mut self, snapshot: SaveSnapshot) -> io::Result<()> {
        match self.sender.try_send(PersistenceRequest::Save(snapshot)) {
            Ok(()) => {
                self.pending_saves = self.pending_saves.saturating_add(1);
                self.last_save = SaveStatus::Pending;
                Ok(())
            }
            Err(mpsc::TrySendError::Full(_)) => Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "a world save is already queued",
            )),
            Err(mpsc::TrySendError::Disconnected(_)) => {
                let error =
                    io::Error::new(io::ErrorKind::BrokenPipe, "world storage worker stopped");
                self.last_save = SaveStatus::Failed(compact_error(&error));
                Err(error)
            }
        }
    }

    pub(crate) fn queue_world_save(
        &mut self,
        player_store: &mut PlayerStore,
        world: &World,
        map_fingerprint: u64,
    ) -> io::Result<()> {
        if self.pending_saves > 0 {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "a world save is already pending",
            ));
        }
        let snapshot = player_store.save_snapshot(world, map_fingerprint)?;
        self.queue_save(snapshot)
    }

    pub(crate) fn poll_completions(&mut self) {
        loop {
            match self.completions.try_recv() {
                Ok(result) => {
                    self.pending_saves = self.pending_saves.saturating_sub(1);
                    match result {
                        Ok(()) => {
                            self.last_save = SaveStatus::Complete;
                            println!("world save completed");
                        }
                        Err(error) => {
                            let message = compact_error(&error);
                            self.last_save = SaveStatus::Failed(message);
                            eprintln!("world save failed: {error}");
                        }
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if self.pending_saves > 0 {
                        self.pending_saves = 0;
                        self.last_save =
                            SaveStatus::Failed(String::from("world_storage_worker_stopped"));
                        eprintln!("world storage worker stopped before completing a save");
                    }
                    break;
                }
            }
        }
    }

    pub(crate) fn has_pending_save(&self) -> bool {
        self.pending_saves != 0
    }

    pub(crate) fn summary(&self) -> String {
        format!(
            "save_pending={} save_last={}",
            self.pending_saves, self.last_save
        )
    }

    pub(crate) fn shutdown(&mut self, snapshot: Option<SaveSnapshot>) -> io::Result<()> {
        let (reply_sender, reply_receiver) = mpsc::sync_channel(1);
        let send_result = self
            .sender
            .send(PersistenceRequest::Shutdown(snapshot, reply_sender))
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "world storage worker stopped"));
        let save_result = if send_result.is_ok() {
            match reply_receiver.recv() {
                Ok(result) => result,
                Err(_) => Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "world storage worker stopped before shutdown completed",
                )),
            }
        } else {
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "world storage worker stopped before shutdown was queued",
            ))
        };
        let join_result = self.join_worker();
        self.poll_completions();
        if let Err(error) = &join_result {
            if send_result.is_err() || save_result.is_err() {
                eprintln!("storage worker join also failed: {error}");
            }
        }
        if let Err(error) = &save_result {
            if send_result.is_err() {
                eprintln!("final save also failed: {error}");
            }
        }
        send_result?;
        save_result?;
        join_result
    }

    fn join_worker(&mut self) -> io::Result<()> {
        if let Some(worker) = self.thread.take() {
            worker
                .join()
                .map_err(|_| io::Error::other("world storage worker panicked"))?;
        }
        Ok(())
    }
}

impl Drop for PersistenceWorker {
    fn drop(&mut self) {
        if self.thread.is_some() {
            let _ = self.sender.send(PersistenceRequest::Stop);
            let _ = self.join_worker();
            self.poll_completions();
        }
    }
}
