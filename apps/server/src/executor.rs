use minimium_telemetry::allocations;
use std::{
    sync::{
        mpsc::{self, SyncSender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
};

type Job = Box<dyn FnOnce() + Send + 'static>;

pub struct Executor {
    sender: Option<SyncSender<Job>>,
    workers: Vec<JoinHandle<()>>,
}

impl Executor {
    /// `lanes` counts the main server lane. One lane executes work inline;
    /// additional lanes are persistent worker threads.
    pub fn new(lanes: usize) -> Self {
        let worker_count = lanes.saturating_sub(1);
        if worker_count == 0 {
            return Self {
                sender: None,
                workers: Vec::new(),
            };
        }
        let (sender, receiver) = mpsc::sync_channel::<Job>(worker_count.saturating_mul(2).max(1));
        let receiver = Arc::new(Mutex::new(receiver));
        let mut workers = Vec::with_capacity(worker_count);
        for index in 0..worker_count {
            let receiver = Arc::clone(&receiver);
            let thread = thread::Builder::new()
                .name(format!("minimium-worker-{index}"))
                .spawn(move || loop {
                    let job = receiver.lock().expect("worker queue poisoned").recv();
                    match job {
                        Ok(job) => job(),
                        Err(_) => break,
                    }
                })
                .expect("failed to create worker thread");
            workers.push(thread);
        }
        Self {
            sender: Some(sender),
            workers,
        }
    }

    pub fn lane_count(&self) -> usize {
        self.workers.len() + 1
    }

    pub fn map<T, R, F>(&self, items: Vec<T>, operation: F) -> Vec<R>
    where
        T: Send + 'static,
        R: Send + 'static,
        F: Fn(T) -> R + Send + Sync + 'static,
    {
        let phase = allocations::current_phase();
        let Some(sender) = &self.sender else {
            return items.into_iter().map(operation).collect();
        };
        let count = items.len();
        if count <= 1 {
            return items.into_iter().map(operation).collect();
        }
        if count <= self.lane_count() {
            let operation = Arc::new(operation);
            let (result_sender, result_receiver) = mpsc::sync_channel(count - 1);
            let mut items = items.into_iter();
            let first_item = items.next().expect("non-empty item list");
            for (index, item) in (1..count).zip(items) {
                let operation = Arc::clone(&operation);
                let result_sender = result_sender.clone();
                sender
                    .send(Box::new(move || {
                        let _phase = allocations::enter(phase);
                        let result = operation(item);
                        let _ = result_sender.send((index, result));
                    }))
                    .expect("worker pool stopped unexpectedly");
            }
            drop(result_sender);
            let mut results: Vec<Option<R>> = (0..count).map(|_| None).collect();
            results[0] = Some(operation(first_item));
            for (index, result) in result_receiver {
                results[index] = Some(result);
            }
            return results
                .into_iter()
                .map(|result| result.expect("worker omitted a result"))
                .collect();
        }

        // Schedule one batch per lane instead of allocating and dispatching one
        // boxed closure for every item. This keeps small servers from paying
        // more synchronization and allocation cost than the work itself.
        let batch_count = (self.workers.len() + 1).min(count);
        let mut batches: Vec<Vec<(usize, T)>> = (0..batch_count)
            .map(|_| Vec::with_capacity(count.div_ceil(batch_count)))
            .collect();
        for (index, item) in items.into_iter().enumerate() {
            batches[index % batch_count].push((index, item));
        }

        let operation = Arc::new(operation);
        let (result_sender, result_receiver) = mpsc::sync_channel(batch_count - 1);
        for batch in batches.drain(1..) {
            let operation = Arc::clone(&operation);
            let result_sender = result_sender.clone();
            sender
                .send(Box::new(move || {
                    let _phase = allocations::enter(phase);
                    let results: Vec<_> = batch
                        .into_iter()
                        .map(|(index, item)| (index, operation(item)))
                        .collect();
                    let _ = result_sender.send(results);
                }))
                .expect("worker pool stopped unexpectedly");
        }
        drop(result_sender);
        let mut results: Vec<Option<R>> = (0..count).map(|_| None).collect();
        for (index, item) in batches[0].drain(..) {
            results[index] = Some(operation(item));
        }
        for batch in result_receiver {
            for (index, result) in batch {
                results[index] = Some(result);
            }
        }
        results
            .into_iter()
            .map(|result| result.expect("worker omitted a result"))
            .collect()
    }
}

impl Drop for Executor {
    fn drop(&mut self) {
        self.sender.take();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}
