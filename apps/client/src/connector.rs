//! Non-blocking TCP connection attempts for UI callers.
//!
//! DNS lookup, sequential socket connections, and local server startup run on
//! one worker thread. Timing out or cancelling an attempt stops its result from
//! being delivered, but does not pretend to cancel blocking OS or startup work:
//! the connector remains busy until that worker exits.

use crate::{bootstrap, options::ThreadLane};
use std::{
    io,
    net::{SocketAddr, TcpStream, ToSocketAddrs},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
        Arc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

/// Stable, monotonically increasing identifier for a connection attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AttemptId(u64);

impl AttemptId {
    /// Returns the numeric value used to order attempts from one connector.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Why an attempt could not be started.
#[derive(Debug)]
pub enum StartError {
    /// A previous worker, including timed-out or cancelled work, is still live.
    Busy { active_attempt: AttemptId },
    /// The connector exhausted its attempt identifier space.
    AttemptIdsExhausted,
    /// The operating system could not create the worker thread.
    WorkerSpawn(io::Error),
}

/// A single event from [`Connector::poll`].
pub enum PollEvent {
    /// No attempt is active.
    Idle,
    /// DNS or socket work is still in flight.
    InProgress {
        attempt_id: AttemptId,
        /// True when the result is already discarded due to timeout/cancel.
        discarding_result: bool,
    },
    /// The common deadline expired. Emitted at most once for this attempt.
    TimedOut { attempt_id: AttemptId },
    /// The matching attempt was cancelled. Emitted at most once.
    Cancelled { attempt_id: AttemptId },
    /// Connection succeeded before the deadline.
    Connected {
        attempt_id: AttemptId,
        stream: TcpStream,
        /// Present when this attempt attached to or started the local server.
        local_server: Option<bootstrap::LocalServer>,
    },
    /// DNS or all socket attempts failed.
    Failed {
        attempt_id: AttemptId,
        error: io::Error,
    },
}

struct WorkerResult {
    completed_at: Instant,
    result: io::Result<ConnectedSocket>,
}

enum WorkerFinalization {
    Accept,
    Discard(WorkerResult),
}

struct ConnectedSocket {
    stream: TcpStream,
    local_server: Option<bootstrap::LocalServer>,
}

enum WorkerOperation {
    Remote(String),
    Local {
        data_dir: PathBuf,
        map_path: Option<PathBuf>,
        world_definition_path: Option<PathBuf>,
        threads: ThreadLane,
    },
}

struct ActiveAttempt {
    id: AttemptId,
    deadline: Instant,
    receiver: Receiver<WorkerResult>,
    worker: Option<JoinHandle<()>>,
    discard: Option<DiscardReason>,
    discard_reported: bool,
    abandoned: Arc<AtomicBool>,
    local: bool,
    finalize: Option<mpsc::Sender<WorkerFinalization>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DiscardReason {
    TimedOut,
    Cancelled,
}

/// Owns at most one DNS/connect worker at a time.
///
/// A timed-out or cancelled worker stays active until it actually finishes;
/// this prevents an uninterruptible resolver call from overlapping a new one.
#[derive(Default)]
pub struct Connector {
    next_id: u64,
    active: Option<ActiveAttempt>,
}

impl Connector {
    /// Starts resolving `address` and connecting to its resolved addresses in
    /// order. `timeout` is one common deadline covering DNS and all attempts.
    pub fn start(
        &mut self,
        address: impl Into<String>,
        timeout: Duration,
    ) -> Result<AttemptId, StartError> {
        self.start_worker(WorkerOperation::Remote(address.into()), timeout)
    }

    /// Starts or attaches to the local server away from the UI thread.
    ///
    /// `timeout` is the connector's common result deadline. The bootstrap
    /// operation itself may continue beyond it; late owned-server guards are
    /// dropped on the worker when their result is discarded.
    pub fn start_local(
        &mut self,
        data_dir: PathBuf,
        map_path: Option<PathBuf>,
        world_definition_path: Option<PathBuf>,
        threads: ThreadLane,
        timeout: Duration,
    ) -> Result<AttemptId, StartError> {
        self.start_worker(
            WorkerOperation::Local {
                data_dir,
                map_path,
                world_definition_path,
                threads,
            },
            timeout,
        )
    }

    fn start_worker(
        &mut self,
        operation: WorkerOperation,
        timeout: Duration,
    ) -> Result<AttemptId, StartError> {
        if let Some(active) = self.active.as_ref() {
            return Err(StartError::Busy {
                active_attempt: active.id,
            });
        }

        let id = self
            .next_id
            .checked_add(1)
            .map(AttemptId)
            .ok_or(StartError::AttemptIdsExhausted)?;
        let now = Instant::now();
        let deadline = now.checked_add(timeout).ok_or_else(|| {
            StartError::WorkerSpawn(io::Error::new(
                io::ErrorKind::InvalidInput,
                "connection timeout is too large",
            ))
        })?;
        let local = matches!(&operation, WorkerOperation::Local { .. });
        let abandoned = Arc::new(AtomicBool::new(false));
        let worker_abandoned = Arc::clone(&abandoned);
        let (sender, receiver) = mpsc::channel();
        let (finalize_sender, finalize_receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name(format!("connector-attempt-{}", id.get()))
            .spawn(move || {
                let mut result = run_operation(operation, deadline);
                let completed_at = Instant::now();
                if worker_abandoned.load(Ordering::SeqCst) {
                    drop(result);
                    result = Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "connection attempt was cancelled",
                    ));
                } else if completed_at > deadline {
                    drop(result);
                    result = Err(timed_out_error());
                }
                if sender
                    .send(WorkerResult {
                        completed_at,
                        result,
                    })
                    .is_ok()
                    && local
                {
                    if let Ok(WorkerFinalization::Discard(result)) = finalize_receiver.recv() {
                        drop(result);
                    }
                }
            })
            .map_err(StartError::WorkerSpawn)?;

        self.next_id = id.get();
        self.active = Some(ActiveAttempt {
            id,
            deadline,
            receiver,
            worker: Some(worker),
            discard: None,
            discard_reported: false,
            abandoned,
            local,
            finalize: local.then_some(finalize_sender),
        });
        Ok(id)
    }

    /// Requests cancellation for the current attempt.
    ///
    /// Cancellation is reported once by `poll`; it cannot interrupt a blocking
    /// system DNS resolver, so `start` remains busy until the worker completes.
    /// Returns false if `attempt_id` is not the active, undiscarded attempt.
    pub fn cancel(&mut self, attempt_id: AttemptId) -> bool {
        match self.active.as_mut() {
            Some(active) if active.id == attempt_id && active.discard.is_none() => {
                active.discard = Some(DiscardReason::Cancelled);
                active.abandoned.store(true, Ordering::SeqCst);
                true
            }
            _ => false,
        }
    }

    /// Wait for a local bootstrap worker on exit so an owned child cannot
    /// outlive this process while its cancelled result is being cleaned up.
    pub fn finish_on_exit(&mut self) {
        if let Some(mut active) = self.active.take() {
            active.abandoned.store(true, Ordering::SeqCst);
            if active.local {
                // The worker waits for this decision after publishing a local
                // result, so send discarded ownership back to it for cleanup.
                match active.receiver.try_recv() {
                    Ok(result) => finalize_discard(&mut active, result),
                    Err(TryRecvError::Empty) => {
                        if let Ok(result) = active.receiver.recv() {
                            finalize_discard(&mut active, result);
                        }
                    }
                    Err(TryRecvError::Disconnected) => {}
                }
                if let Some(worker) = active.worker.take() {
                    let _ = worker.join();
                }
            }
        }
    }

    /// Polls without blocking. Timeout/cancel events are delivered once; while
    /// their worker is still running later calls return `InProgress` with
    /// `discarding_result: true`, then `Idle` once the worker exits.
    pub fn poll(&mut self) -> PollEvent {
        let Some(active) = self.active.as_mut() else {
            return PollEvent::Idle;
        };

        if let Some(reason) = active.discard {
            let finished = take_finished(active).is_some();
            if !active.discard_reported {
                active.discard_reported = true;
                let id = active.id;
                if finished {
                    self.active = None;
                }
                return match reason {
                    DiscardReason::TimedOut => PollEvent::TimedOut { attempt_id: id },
                    DiscardReason::Cancelled => PollEvent::Cancelled { attempt_id: id },
                };
            }
            if finished {
                self.active = None;
                return PollEvent::Idle;
            }
            return PollEvent::InProgress {
                attempt_id: active.id,
                discarding_result: true,
            };
        }

        match active.receiver.try_recv() {
            Ok(worker_result) => {
                let id = active.id;
                let on_time = worker_result.completed_at <= active.deadline;
                if !on_time {
                    if active.local {
                        active.discard = Some(DiscardReason::TimedOut);
                        active.discard_reported = true;
                        finalize_discard(active, worker_result);
                    } else {
                        drop(worker_result);
                        self.active = None;
                    }
                    return PollEvent::TimedOut { attempt_id: id };
                }
                let result = worker_result.result;
                if active.local {
                    if let Some(finalize) = active.finalize.take() {
                        let _ = finalize.send(WorkerFinalization::Accept);
                    }
                }
                self.active = None;
                match result {
                    Ok(connected) => PollEvent::Connected {
                        attempt_id: id,
                        stream: connected.stream,
                        local_server: connected.local_server,
                    },
                    Err(error) => PollEvent::Failed {
                        attempt_id: id,
                        error,
                    },
                }
            }
            Err(TryRecvError::Empty) if Instant::now() >= active.deadline => {
                active.discard = Some(DiscardReason::TimedOut);
                active.discard_reported = true;
                active.abandoned.store(true, Ordering::SeqCst);
                PollEvent::TimedOut {
                    attempt_id: active.id,
                }
            }
            Err(TryRecvError::Empty) => PollEvent::InProgress {
                attempt_id: active.id,
                discarding_result: false,
            },
            Err(TryRecvError::Disconnected) => {
                if !active.worker.as_ref().is_some_and(JoinHandle::is_finished) {
                    return PollEvent::InProgress {
                        attempt_id: active.id,
                        discarding_result: false,
                    };
                }
                let id = active.id;
                let panicked = active
                    .worker
                    .take()
                    .is_some_and(|worker| worker.join().is_err());
                self.active = None;
                PollEvent::Failed {
                    attempt_id: id,
                    error: io::Error::other(if panicked {
                        "connection worker panicked before returning a result"
                    } else {
                        "connection worker ended without returning a result"
                    }),
                }
            }
        }
    }
}

fn take_finished(active: &mut ActiveAttempt) -> Option<()> {
    match active.receiver.try_recv() {
        Ok(result) => {
            if active.local {
                finalize_discard(active, result);
                None
            } else {
                drop(result);
                Some(())
            }
        }
        Err(TryRecvError::Disconnected)
            if active.worker.as_ref().is_some_and(JoinHandle::is_finished) =>
        {
            if let Some(worker) = active.worker.take() {
                let _ = worker.join();
            }
            Some(())
        }
        Err(TryRecvError::Disconnected) => None,
        Err(TryRecvError::Empty) if active.worker.as_ref().is_some_and(JoinHandle::is_finished) => {
            if let Some(worker) = active.worker.take() {
                let _ = worker.join();
            }
            Some(())
        }
        Err(TryRecvError::Empty) => None,
    }
}

/// Returns a discarded local result to its worker so its guard is dropped
/// away from the polling thread.
fn finalize_discard(active: &mut ActiveAttempt, result: WorkerResult) {
    if let Some(finalize) = active.finalize.take() {
        let _ = finalize.send(WorkerFinalization::Discard(result));
    }
}

fn run_operation(operation: WorkerOperation, deadline: Instant) -> io::Result<ConnectedSocket> {
    if Instant::now() >= deadline {
        return Err(timed_out_error());
    }
    match operation {
        WorkerOperation::Remote(address) => Ok(ConnectedSocket {
            stream: connect_all(&address, deadline)?,
            local_server: None,
        }),
        WorkerOperation::Local {
            data_dir,
            map_path,
            world_definition_path,
            threads,
        } => {
            let (stream, local_server) = bootstrap::connect_or_start_until(
                &data_dir,
                map_path.as_deref(),
                world_definition_path.as_deref(),
                threads,
                deadline,
            )
            .map_err(|error| io::Error::other(format!("local server startup failed: {error}")))?;
            Ok(ConnectedSocket {
                stream,
                local_server: Some(local_server),
            })
        }
    }
}

fn connect_all(address: &str, deadline: Instant) -> io::Result<TcpStream> {
    if Instant::now() >= deadline {
        return Err(timed_out_error());
    }

    let addresses: Vec<SocketAddr> = address.to_socket_addrs()?.collect();
    if addresses.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::AddrNotAvailable,
            format!("no socket addresses resolved for {address:?}"),
        ));
    }

    let mut last_error = None;
    let mut attempted = 0usize;
    for socket in addresses {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(timed_out_error());
        }
        attempted += 1;
        match TcpStream::connect_timeout(&socket, remaining) {
            Ok(stream) if Instant::now() < deadline => return Ok(stream),
            Ok(_) => return Err(timed_out_error()),
            Err(_error) if Instant::now() >= deadline => return Err(timed_out_error()),
            Err(error) => {
                last_error = Some(io::Error::new(
                    error.kind(),
                    format!("connection to {socket} failed: {error}"),
                ));
            }
        }
    }

    Err(last_error.unwrap_or_else(|| {
        io::Error::new(
            io::ErrorKind::AddrNotAvailable,
            format!("no socket address could be attempted for {address:?} ({attempted} tried)"),
        )
    }))
}

fn timed_out_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::TimedOut,
        "connection deadline expired during DNS or socket connection",
    )
}
