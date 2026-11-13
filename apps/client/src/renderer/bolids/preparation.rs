//! One application-owned worker; bounded queues never block the render thread.
use minimium_protocol::{BolidEvent, WorldContext};
use minimium_renderer::milky_way_gpu::{BolidEvolution, BolidGpuEvent, MAX_BOLIDS};
use std::sync::mpsc::{self, Receiver, SyncSender};

#[derive(Clone, Copy, PartialEq)]
pub(in crate::renderer) struct Key {
    pub context: WorldContext,
    pub event: BolidEvent,
    pub radius: f64,
    pub scale: f64,
}

struct Job {
    key: Key,
    input: BolidGpuEvent,
}

pub(in crate::renderer) struct BolidWorker {
    requests: SyncSender<Job>,
    completed: Receiver<(Key, BolidEvolution)>,
    pending: Vec<Key>,
}

impl BolidWorker {
    pub fn new() -> std::io::Result<Self> {
        let (requests, jobs) = mpsc::sync_channel::<Job>(MAX_BOLIDS);
        let (results, completed) = mpsc::sync_channel(MAX_BOLIDS);
        std::thread::Builder::new()
            .name("bolid-history".into())
            .spawn(move || {
                while let Ok(job) = jobs.recv() {
                    let model = BolidEvolution::new(&job.input, job.key.radius, job.key.scale);
                    if results.send((job.key, model)).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self {
            requests,
            completed,
            pending: Vec::with_capacity(MAX_BOLIDS),
        })
    }

    pub fn request(&mut self, key: Key, input: BolidGpuEvent) {
        if self.pending.len() >= MAX_BOLIDS || self.pending.contains(&key) {
            return;
        }
        if self.requests.try_send(Job { key, input }).is_ok() {
            self.pending.push(key);
        }
    }

    pub fn poll(&mut self) -> Option<(Key, BolidEvolution)> {
        let result = self.completed.try_recv().ok()?;
        self.pending.retain(|key| *key != result.0);
        Some(result)
    }
}
