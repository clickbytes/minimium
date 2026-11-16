//! One queued job per family, with a separate host request/completion phase.
use minimium_client_runtime::{
    ContentCancelStatus, ContentCancellation, ContentJobCompletion, ContentJobHandle,
    ContentJobPoll, ContentJobRequest, ContentJobResult, ContentJobTicket, ContentJobsPort,
    ContentShutdownStatus, ContentSubmitError, SessionFailure,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard, TryLockError,
    },
    time::Instant,
};

struct Slot {
    handle: ContentJobHandle,
    request: Option<ContentJobRequest>,
    working: bool,
    result: Option<ContentJobResult>,
}

impl Slot {
    fn finished(&self) -> bool {
        !self.working || self.result.is_some()
    }
}

#[derive(Default)]
struct Slots {
    world: Option<Slot>,
    material: Option<Slot>,
}

impl Slots {
    fn slot(&mut self, ticket: ContentJobTicket) -> &mut Option<Slot> {
        match ticket {
            ContentJobTicket::World(_) => &mut self.world,
            ContentJobTicket::Material(_) => &mut self.material,
        }
    }

    fn reap_cancelled(&mut self, retained: Option<ContentJobTicket>) {
        for slot in [&mut self.world, &mut self.material] {
            if slot.as_ref().is_some_and(|job| {
                Some(job.handle.ticket()) != retained && job.handle.is_cancelled() && job.finished()
            }) {
                *slot = None;
            }
        }
    }

    fn close(&mut self) {
        for job in [&self.world, &self.material].into_iter().flatten() {
            job.handle.request_cancel();
        }
        self.reap_cancelled(None);
    }
}

#[derive(Default)]
struct Shared {
    slots: Mutex<Slots>,
    closed: AtomicBool,
}

impl Shared {
    fn try_slots(&self) -> Option<MutexGuard<'_, Slots>> {
        match self.slots.try_lock() {
            Ok(slots) => Some(slots),
            Err(TryLockError::WouldBlock) => None,
            Err(TryLockError::Poisoned(error)) => Some(error.into_inner()),
        }
    }
}

#[derive(Clone)]
pub struct ContentPeer(Arc<Shared>);

impl ContentPeer {
    /// The host owns any preparation after taking this request. No work happens
    /// inside submit; the handle distinguishes cancellation from completion.
    pub fn take_request(&self) -> Option<(ContentJobHandle, ContentJobRequest)> {
        if self.0.closed.load(Ordering::Acquire) {
            return None;
        }
        let mut slots = self.0.try_slots()?;
        if self.0.closed.load(Ordering::Acquire) {
            return None;
        }
        slots.reap_cancelled(None);
        let slots = &mut *slots;
        for job in [&mut slots.world, &mut slots.material]
            .into_iter()
            .flatten()
        {
            if let Some(request) = job.request.take() {
                job.working = true;
                return Some((job.handle.clone(), request));
            }
        }
        None
    }

    /// Completion also acknowledges that cancelled host work has stopped; such
    /// a result is discarded and releases its slot instead of being published.
    pub fn complete(
        &self,
        ticket: ContentJobTicket,
        result: ContentJobResult,
    ) -> Result<(), ContentSubmitError> {
        let mut slots = self.0.try_slots().ok_or(ContentSubmitError::Busy)?;
        let slot = slots.slot(ticket);
        let Some(job) = slot.as_mut().filter(|job| job.handle.ticket() == ticket) else {
            return Err(ContentSubmitError::Rejected(SessionFailure::new(
                "example.content_ticket_unknown",
                "completion has no matching request",
            )));
        };
        if !job.working || job.result.is_some() {
            return Err(ContentSubmitError::Rejected(SessionFailure::new(
                "example.content_not_working",
                "completion does not match active host work",
            )));
        }
        if job.handle.is_cancelled() || self.0.closed.load(Ordering::Acquire) {
            *slot = None;
        } else {
            job.result = Some(result);
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct LocalContentJobs(Arc<Shared>);

impl LocalContentJobs {
    pub fn peer(&self) -> ContentPeer {
        ContentPeer(self.0.clone())
    }
}

impl ContentJobsPort for LocalContentJobs {
    fn submit(&self, request: ContentJobRequest) -> Result<ContentJobHandle, ContentSubmitError> {
        if self.0.closed.load(Ordering::Acquire) {
            return Err(ContentSubmitError::Closed);
        }
        let mut slots = self.0.try_slots().ok_or(ContentSubmitError::Busy)?;
        if self.0.closed.load(Ordering::Acquire) {
            return Err(ContentSubmitError::Closed);
        }
        slots.reap_cancelled(None);
        let ticket = request.ticket();
        let slot = slots.slot(ticket);
        if slot.is_some() {
            return Err(ContentSubmitError::Busy);
        }
        let handle = ContentJobHandle::new(ticket, ContentCancellation::default());
        *slot = Some(Slot {
            handle: handle.clone(),
            request: Some(request),
            working: false,
            result: None,
        });
        Ok(handle)
    }

    fn poll(&self, ticket: ContentJobTicket) -> ContentJobPoll {
        let Some(mut slots) = self.0.try_slots() else {
            return ContentJobPoll::Pending;
        };
        slots.reap_cancelled(Some(ticket));
        let slot = slots.slot(ticket);
        let Some(job) = slot.as_mut().filter(|job| job.handle.ticket() == ticket) else {
            return ContentJobPoll::Unknown;
        };
        if job.handle.is_cancelled() && job.finished() {
            *slot = None;
            return ContentJobPoll::Cancelled;
        }
        let Some(result) = job.result.take() else {
            return ContentJobPoll::Pending;
        };
        *slot = None;
        ContentJobPoll::Complete(ContentJobCompletion { ticket, result })
    }

    fn cancel(&self, handle: &ContentJobHandle) -> ContentCancelStatus {
        handle.request_cancel();
        let Some(mut slots) = self.0.try_slots() else {
            return ContentCancelStatus::Requested;
        };
        let slot = slots.slot(handle.ticket());
        let Some(job) = slot
            .as_mut()
            .filter(|job| job.handle.ticket() == handle.ticket())
        else {
            return ContentCancelStatus::Unknown;
        };
        job.handle.request_cancel();
        if job.finished() {
            *slot = None;
            ContentCancelStatus::Finished
        } else {
            ContentCancelStatus::Requested
        }
    }

    fn shutdown_until(&self, _deadline: Instant) -> ContentShutdownStatus {
        self.0.closed.store(true, Ordering::Release);
        let Some(mut slots) = self.0.try_slots() else {
            return ContentShutdownStatus::Pending;
        };
        slots.close();
        if slots.world.is_none() && slots.material.is_none() {
            ContentShutdownStatus::Complete
        } else {
            ContentShutdownStatus::Pending
        }
    }
}

impl Drop for LocalContentJobs {
    fn drop(&mut self) {
        let _ = self.shutdown_until(Instant::now());
    }
}
