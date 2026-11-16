//! Application-owned execution of bounded world and material preparation.
mod workers;

use minimium_client_runtime::{
    ContentCancelStatus, ContentCancellation, ContentJobCompletion, ContentJobHandle,
    ContentJobPoll, ContentJobRequest, ContentJobResult, ContentJobTicket, ContentJobsPort,
    ContentShutdownStatus, ContentSubmitError, SessionFailure, WorldContentResolver,
};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard, TryLockError,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

/// Shared by all sessions of one controller. Each family has one slot, including
/// finished results and cancellation-requested work that has not exited yet.
pub(crate) struct HostContentJobs {
    state: Mutex<State>,
    closed: AtomicBool,
    cache_dir: PathBuf,
    cache_limit_bytes: u64,
    resolver: Arc<dyn WorldContentResolver>,
}

#[derive(Default)]
struct State {
    world: Option<Slot>,
    material: Option<Slot>,
}

struct Slot {
    handle: ContentJobHandle,
    worker: JoinHandle<ContentJobResult>,
}

impl Slot {
    fn cancel(&mut self) {
        self.handle.request_cancel();
    }

    fn finish(self) -> ContentJobCompletion {
        let ticket = self.handle.ticket();
        let result = self.worker.join().unwrap_or_else(|_| match ticket {
            ContentJobTicket::World(ticket) => ContentJobResult::World(Err(SessionFailure::new(
                "session.world_content_worker_stopped",
                "world-content worker stopped unexpectedly",
            )
            .with_content_ticket(ticket))),
            ContentJobTicket::Material(_) => ContentJobResult::Material(Err(SessionFailure::new(
                "session.material_worker_failed",
                "material worker thread terminated unexpectedly",
            ))),
        });
        ContentJobCompletion { ticket, result }
    }
}

impl State {
    fn slot(&mut self, ticket: ContentJobTicket) -> &mut Option<Slot> {
        match ticket {
            ContentJobTicket::World(_) => &mut self.world,
            ContentJobTicket::Material(_) => &mut self.material,
        }
    }

    fn cancel_all(&mut self) {
        for slot in [&mut self.world, &mut self.material].into_iter().flatten() {
            slot.cancel();
        }
    }

    fn reap_cancelled(&mut self, retained: Option<ContentJobTicket>) {
        for slot in [&mut self.world, &mut self.material] {
            if slot.as_ref().is_some_and(|job| {
                Some(job.handle.ticket()) != retained
                    && job.handle.is_cancelled()
                    && job.worker.is_finished()
            }) {
                // is_finished is the only path to joining on an owner poll.
                drop(slot.take().map(Slot::finish));
            }
        }
    }

    fn is_empty(&self) -> bool {
        self.world.is_none() && self.material.is_none()
    }
}

impl HostContentJobs {
    pub(crate) fn new(
        cache_dir: PathBuf,
        cache_limit_bytes: u64,
        resolver: Arc<dyn WorldContentResolver>,
    ) -> Self {
        Self {
            state: Mutex::new(State::default()),
            closed: AtomicBool::new(false),
            cache_dir,
            cache_limit_bytes: cache_limit_bytes.max(1),
            resolver,
        }
    }

    fn try_state(&self) -> Option<MutexGuard<'_, State>> {
        match self.state.try_lock() {
            Ok(state) => Some(state),
            Err(TryLockError::WouldBlock) => None,
            Err(TryLockError::Poisoned(error)) => Some(error.into_inner()),
        }
    }

    fn start(&self, request: ContentJobRequest) -> Result<Slot, ContentSubmitError> {
        let ticket = request.ticket();
        let handle = ContentJobHandle::new(ticket, ContentCancellation::default());
        let worker = match request {
            ContentJobRequest::World(request) => {
                let cancel = handle.cancellation();
                let resolver = self.resolver.clone();
                let worker = std::thread::Builder::new()
                    .name("world-content-prepare".into())
                    .spawn(move || workers::world(request, resolver, cancel))
                    .map_err(|error| {
                        ContentSubmitError::Rejected(
                            SessionFailure::caused_by(
                                "session.world_content_worker_start_failed",
                                "could not start world-content worker",
                                error,
                            )
                            .with_content_ticket(request.ticket),
                        )
                    })?;
                worker
            }
            ContentJobRequest::Material(request) => {
                let cancel = handle.cancellation();
                let cache = self.cache_dir.clone();
                let budget = self.cache_limit_bytes;
                let worker = std::thread::Builder::new()
                    .name("material-download".into())
                    .spawn(move || workers::material(request, cache, budget, cancel))
                    .map_err(|error| {
                        ContentSubmitError::Rejected(SessionFailure::from_asset_error(
                            minimium_asset_io::AssetIoError::io(
                                "start material download worker",
                                error,
                            ),
                        ))
                    })?;
                worker
            }
        };
        Ok(Slot { handle, worker })
    }
}

impl ContentJobsPort for HostContentJobs {
    fn submit(&self, request: ContentJobRequest) -> Result<ContentJobHandle, ContentSubmitError> {
        if self.closed.load(Ordering::Acquire) {
            return Err(ContentSubmitError::Closed);
        }
        let mut state = self.try_state().ok_or(ContentSubmitError::Busy)?;
        if self.closed.load(Ordering::Acquire) {
            return Err(ContentSubmitError::Closed);
        }
        state.reap_cancelled(None);
        let ticket = request.ticket();
        let slot = state.slot(ticket);
        if slot.is_some() {
            return Err(ContentSubmitError::Busy);
        }
        let job = self.start(request)?;
        let handle = job.handle.clone();
        *slot = Some(job);
        Ok(handle)
    }

    fn poll(&self, ticket: ContentJobTicket) -> ContentJobPoll {
        let Some(mut state) = self.try_state() else {
            return ContentJobPoll::Pending;
        };
        state.reap_cancelled(Some(ticket));
        let slot = state.slot(ticket);
        let Some(job) = slot.as_ref().filter(|job| job.handle.ticket() == ticket) else {
            return ContentJobPoll::Unknown;
        };
        if !job.worker.is_finished() {
            return ContentJobPoll::Pending;
        }
        let job = slot.take().expect("matching finished slot exists");
        if job.handle.is_cancelled() {
            drop(job.finish());
            ContentJobPoll::Cancelled
        } else {
            ContentJobPoll::Complete(job.finish())
        }
    }

    fn cancel(&self, handle: &ContentJobHandle) -> ContentCancelStatus {
        handle.request_cancel();
        let ticket = handle.ticket();
        let Some(mut state) = self.try_state() else {
            return ContentCancelStatus::Requested;
        };
        state.reap_cancelled(Some(ticket));
        let slot = state.slot(ticket);
        let Some(job) = slot.as_mut().filter(|job| job.handle.ticket() == ticket) else {
            return ContentCancelStatus::Unknown;
        };
        job.cancel();
        if job.worker.is_finished() {
            drop(slot.take().map(Slot::finish));
            ContentCancelStatus::Finished
        } else {
            ContentCancelStatus::Requested
        }
    }

    fn shutdown_until(&self, deadline: Instant) -> ContentShutdownStatus {
        self.closed.store(true, Ordering::Release);
        loop {
            if let Some(mut state) = self.try_state() {
                state.cancel_all();
                state.reap_cancelled(None);
                if state.is_empty() {
                    return ContentShutdownStatus::Complete;
                }
            }
            if Instant::now() >= deadline {
                return ContentShutdownStatus::Pending;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

impl Drop for HostContentJobs {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Release);
        let state = self
            .state
            .get_mut()
            .unwrap_or_else(|error| error.into_inner());
        state.cancel_all();
        // Dropping running handles detaches without waiting. Their closures own
        // cache/resolver/cancellation and any in-flight package leases until exit.
    }
}
