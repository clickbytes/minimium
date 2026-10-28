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

pub(crate) struct PersistenceWorker {
    sender: mpsc::SyncSender<PersistenceRequest>,
    completions: mpsc::Receiver<io::Result<()>>,
    thread: Option<thread::JoinHandle<()>>,
    pub(crate) pending_saves: usize,
    last_save: String,
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
            last_save: String::from("never"),
        })
    }

    pub(crate) fn queue_save(&mut self, snapshot: SaveSnapshot) -> io::Result<()> {
        match self.sender.try_send(PersistenceRequest::Save(snapshot)) {
            Ok(()) => {
                self.pending_saves = self.pending_saves.saturating_add(1);
                self.last_save = String::from("pending");
                Ok(())
            }
            Err(mpsc::TrySendError::Full(_)) => Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "a world save is already queued",
            )),
            Err(mpsc::TrySendError::Disconnected(_)) => Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "world storage worker stopped",
            )),
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
        while let Ok(result) = self.completions.try_recv() {
            self.pending_saves = self.pending_saves.saturating_sub(1);
            match result {
                Ok(()) => {
                    self.last_save = String::from("ok");
                    println!("world save completed");
                }
                Err(error) => {
                    let message = compact_error(&error);
                    self.last_save = format!("error_{message}");
                    eprintln!("world save failed: {error}");
                }
            }
        }
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
        send_result?;
        join_result?;
        save_result
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
