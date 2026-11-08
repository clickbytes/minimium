//! Asynchronous bounded JSONL diagnostics backend.

mod run;
mod writer;

use minimium_diagnostics::{DiagnosticEvent, DiagnosticSink, DomainId, DomainLogger, Level};
use std::{
    collections::HashMap,
    io,
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

const NORMAL_QUEUE_CAPACITY: usize = 4096;
const PRIORITY_QUEUE_CAPACITY: usize = 128;
const RECEIVE_POLL: Duration = Duration::from_millis(10);

struct QueueSink {
    normal: SyncSender<DiagnosticEvent>,
    priority: SyncSender<DiagnosticEvent>,
    dropped_normal: Arc<AtomicU64>,
    dropped_priority: Arc<AtomicU64>,
    minimum_levels: HashMap<DomainId, Level>,
    stopping: Arc<AtomicBool>,
}

impl DiagnosticSink for QueueSink {
    fn emit(&self, event: DiagnosticEvent) {
        if self.stopping.load(Ordering::Acquire) {
            return;
        }
        let minimum = self
            .minimum_levels
            .get(&event.domain())
            .copied()
            .unwrap_or(Level::Info);
        if event.level() < minimum {
            return;
        }
        let (sender, dropped) = if event.level() >= Level::Warn {
            (&self.priority, &self.dropped_priority)
        } else {
            (&self.normal, &self.dropped_normal)
        };
        if let Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) = sender.try_send(event) {
            dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Owns the single writer worker, active run directory and shared nonblocking sink.
pub struct DiagnosticsRuntime {
    sink: Option<Arc<QueueSink>>,
    worker: Option<JoinHandle<()>>,
    run_id: String,
    worker_error: Arc<Mutex<Option<io::Error>>>,
    stopping: Arc<AtomicBool>,
}

impl DiagnosticsRuntime {
    pub fn start(log_root: &Path) -> io::Result<Self> {
        Self::start_with_options(log_root, None, HashMap::new())
    }

    pub fn start_with_parent(log_root: &Path, parent_run_id: Option<String>) -> io::Result<Self> {
        Self::start_with_options(log_root, parent_run_id, HashMap::new())
    }

    pub fn start_with_options(
        log_root: &Path,
        parent_run_id: Option<String>,
        minimum_levels: HashMap<DomainId, Level>,
    ) -> io::Result<Self> {
        let run = run::RunDirectory::create(log_root, parent_run_id)?;
        let run_id = run.run_id.clone();
        let (normal_tx, normal_rx) = mpsc::sync_channel(NORMAL_QUEUE_CAPACITY);
        let (priority_tx, priority_rx) = mpsc::sync_channel(PRIORITY_QUEUE_CAPACITY);
        let dropped_normal = Arc::new(AtomicU64::new(0));
        let dropped_priority = Arc::new(AtomicU64::new(0));
        let worker_error = Arc::new(Mutex::new(None));
        let stopping = Arc::new(AtomicBool::new(false));
        let worker_normal_dropped = dropped_normal.clone();
        let worker_priority_dropped = dropped_priority.clone();
        let worker_error_copy = worker_error.clone();
        let worker_stopping = stopping.clone();
        let worker = thread::Builder::new()
            .name("diagnostics-writer".to_owned())
            .spawn(move || {
                drain_events(
                    normal_rx,
                    priority_rx,
                    writer::RunWriter::new(run),
                    worker_normal_dropped,
                    worker_priority_dropped,
                    worker_error_copy,
                    worker_stopping,
                )
            })?;
        Ok(Self {
            sink: Some(Arc::new(QueueSink {
                normal: normal_tx,
                priority: priority_tx,
                dropped_normal,
                dropped_priority,
                minimum_levels,
                stopping: stopping.clone(),
            })),
            worker: Some(worker),
            run_id,
            worker_error,
            stopping,
        })
    }

    pub fn logger(&self, domain: DomainId) -> DomainLogger {
        DomainLogger::new(
            domain,
            self.sink
                .as_ref()
                .expect("diagnostics runtime is active")
                .clone(),
        )
    }

    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    pub fn dropped_records(&self) -> (u64, u64) {
        self.sink.as_ref().map_or((0, 0), |sink| {
            (
                sink.dropped_normal.load(Ordering::Relaxed),
                sink.dropped_priority.load(Ordering::Relaxed),
            )
        })
    }

    pub fn take_worker_error(&self) -> Option<io::Error> {
        self.worker_error
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
    }

    /// Close the process sink and wait for queued records to flush up to the supplied deadline.
    pub fn shutdown_until(&mut self, deadline: std::time::Instant) -> io::Result<()> {
        self.stopping.store(true, Ordering::Release);
        self.sink.take();
        while self
            .worker
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let Some(worker) = self.worker.take() else {
            return Ok(());
        };
        if !worker.is_finished() {
            drop(worker);
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "diagnostics writer exceeded shutdown deadline",
            ));
        }
        worker
            .join()
            .map_err(|_| io::Error::other("diagnostics writer panicked"))?;
        if let Some(error) = self.take_worker_error() {
            return Err(error);
        }
        Ok(())
    }
}

impl Drop for DiagnosticsRuntime {
    fn drop(&mut self) {
        let _ = self.shutdown_until(std::time::Instant::now() + std::time::Duration::from_secs(2));
    }
}

fn drain_events(
    normal: Receiver<DiagnosticEvent>,
    priority: Receiver<DiagnosticEvent>,
    mut writer: writer::RunWriter,
    dropped_normal: Arc<AtomicU64>,
    dropped_priority: Arc<AtomicU64>,
    worker_error: Arc<Mutex<Option<io::Error>>>,
    stopping: Arc<AtomicBool>,
) {
    let mut observed_normal = 0;
    let mut observed_priority = 0;
    let mut normal_closed = false;
    let mut priority_closed = false;
    loop {
        let mut event = None;
        if !priority_closed {
            match priority.try_recv() {
                Ok(next) => event = Some(next),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => priority_closed = true,
            }
        }
        if event.is_none() && !normal_closed {
            match normal.recv_timeout(RECEIVE_POLL) {
                Ok(next) => event = Some(next),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => normal_closed = true,
            }
        }
        if event.is_none() && normal_closed && !priority_closed {
            match priority.recv_timeout(RECEIVE_POLL) {
                Ok(next) => event = Some(next),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => priority_closed = true,
            }
        }
        let Some(event) = event else {
            if (normal_closed && priority_closed) || stopping.load(Ordering::Acquire) {
                break;
            }
            continue;
        };
        let current_normal = dropped_normal.load(Ordering::Relaxed);
        let current_priority = dropped_priority.load(Ordering::Relaxed);
        if current_normal != observed_normal || current_priority != observed_priority {
            if let Err(error) = writer.write_loss_summary(
                current_normal.saturating_sub(observed_normal),
                current_priority.saturating_sub(observed_priority),
            ) {
                record_writer_error(&worker_error, error);
                break;
            }
            observed_normal = current_normal;
            observed_priority = current_priority;
        }
        if let Err(error) = writer.write_event(&event) {
            record_writer_error(&worker_error, error);
            break;
        }
    }
    let current_normal = dropped_normal.load(Ordering::Relaxed);
    let current_priority = dropped_priority.load(Ordering::Relaxed);
    if current_normal != observed_normal || current_priority != observed_priority {
        if let Err(error) = writer.write_loss_summary(
            current_normal.saturating_sub(observed_normal),
            current_priority.saturating_sub(observed_priority),
        ) {
            record_writer_error(&worker_error, error);
        }
    }
    if let Err(error) = writer.flush() {
        record_writer_error(&worker_error, error);
    }
}

fn record_writer_error(slot: &Mutex<Option<io::Error>>, error: io::Error) {
    eprintln!("diagnostics writer failed: {error}");
    let mut slot = slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if slot.is_none() {
        *slot = Some(error);
    }
}

pub(super) fn unix_time_ms() -> io::Result<u128> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}
