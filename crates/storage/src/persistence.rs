use super::{
    player_store::{PlayerStore, SaveSnapshot},
    WorldSaveInput,
};
use minimium_environment::time::SavedClockState;
use minimium_telemetry::allocations::{self, Phase};
use std::{io, path::PathBuf, sync::mpsc, thread};

mod status;

use status::SaveBookkeeping;

enum PersistenceRequest {
    Save(SaveSnapshot),
    Shutdown(Option<SaveSnapshot>, mpsc::SyncSender<io::Result<()>>),
    Stop,
}

pub struct PersistenceWorker {
    sender: mpsc::SyncSender<PersistenceRequest>,
    completions: mpsc::Receiver<io::Result<()>>,
    thread: Option<thread::JoinHandle<()>>,
    status: SaveBookkeeping,
}

impl PersistenceWorker {
    pub fn spawn(data_dir: PathBuf) -> io::Result<Self> {
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
            status: SaveBookkeeping::default(),
        })
    }

    pub fn queue_save(&mut self, snapshot: SaveSnapshot) -> io::Result<()> {
        match self.sender.try_send(PersistenceRequest::Save(snapshot)) {
            Ok(()) => {
                self.status.record_queued();
                Ok(())
            }
            Err(mpsc::TrySendError::Full(_)) => Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "a world save is already queued",
            )),
            Err(mpsc::TrySendError::Disconnected(_)) => {
                let error = worker_stopped();
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
    ) -> io::Result<()> {
        if self.status.has_pending_save() {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "a world save is already pending",
            ));
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

    pub fn shutdown(&mut self, snapshot: Option<SaveSnapshot>) -> io::Result<()> {
        let (reply_sender, reply_receiver) = mpsc::sync_channel(1);
        let send_result = self.send_shutdown(snapshot, reply_sender);
        let save_result = Self::receive_shutdown_result(&send_result, reply_receiver);
        let join_result = self.join_worker();
        self.poll_completions();
        Self::report_shutdown_errors(&send_result, &save_result, &join_result);
        send_result?;
        save_result?;
        join_result
    }

    fn send_shutdown(
        &self,
        snapshot: Option<SaveSnapshot>,
        reply: mpsc::SyncSender<io::Result<()>>,
    ) -> io::Result<()> {
        self.sender
            .send(PersistenceRequest::Shutdown(snapshot, reply))
            .map_err(|_| worker_stopped())
    }

    fn receive_shutdown_result(
        send_result: &io::Result<()>,
        reply: mpsc::Receiver<io::Result<()>>,
    ) -> io::Result<()> {
        if send_result.is_ok() {
            match reply.recv() {
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
        }
    }

    fn report_shutdown_errors(
        send_result: &io::Result<()>,
        save_result: &io::Result<()>,
        join_result: &io::Result<()>,
    ) {
        if let Err(error) = join_result {
            if send_result.is_err() || save_result.is_err() {
                eprintln!("storage worker join also failed: {error}");
            }
        }
        if let Err(error) = save_result {
            if send_result.is_err() {
                eprintln!("final save also failed: {error}");
            }
        }
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

fn worker_stopped() -> io::Error {
    io::Error::new(io::ErrorKind::BrokenPipe, "world storage worker stopped")
}
