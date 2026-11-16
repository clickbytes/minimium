use super::receive::WorldContentProgress;
use super::*;
use crate::{
    ContentJobPoll, ContentJobRequest, ContentJobResult, ContentJobTicket, ContentSubmitError,
};

impl ClientSession {
    pub(super) fn world_content_pending(&self) -> bool {
        self.world_content_request.is_some() && self.world_content_result.is_none()
    }

    pub(super) fn cancel_world_content_job(&mut self) {
        if let Some(handle) = self.world_content_job.as_ref() {
            handle.request_cancel();
            self.content_jobs.port().cancel(handle);
        }
    }

    pub(super) fn pump_world_content_job(&mut self) {
        let wanted = self
            .world_content_request
            .map(|request| ContentJobTicket::World(request.ticket));
        if let Some(handle) = self.world_content_job.as_ref() {
            if wanted != Some(handle.ticket()) {
                handle.request_cancel();
                self.content_jobs.port().cancel(handle);
                self.world_content_result = None;
            }
            if self.world_content_result.is_some() {
                return;
            }
            match self.content_jobs.port().poll(handle.ticket()) {
                ContentJobPoll::Pending => return,
                ContentJobPoll::Complete(completion) => {
                    if completion.ticket != handle.ticket() {
                        return;
                    }
                    if !handle.is_cancelled() && wanted == Some(completion.ticket) {
                        self.world_content_result = Some(match completion.result {
                            ContentJobResult::World(result) => result,
                            ContentJobResult::Material(_) => Err(SessionFailure::new(
                                "session.world_content_result_kind_mismatch",
                                "content service returned a material result for world preparation",
                            )),
                        });
                        return;
                    }
                }
                ContentJobPoll::Cancelled | ContentJobPoll::Unknown => {
                    if !handle.is_cancelled() && wanted == Some(handle.ticket()) {
                        self.world_content_result = Some(Err(SessionFailure::new(
                            "session.world_content_worker_stopped",
                            "world-content work stopped unexpectedly",
                        )));
                        return;
                    }
                }
            }
            self.world_content_job = None;
        }
        let Some(request) = self.world_content_request else {
            return;
        };
        if self.world_content_result.is_some() {
            return;
        }
        match self
            .content_jobs
            .port()
            .submit(ContentJobRequest::World(request))
        {
            Ok(handle) if handle.ticket() == ContentJobTicket::World(request.ticket) => {
                self.world_content_job = Some(handle);
            }
            Ok(handle) => {
                handle.request_cancel();
                self.content_jobs.port().cancel(&handle);
                self.world_content_result = Some(Err(SessionFailure::new(
                    "session.world_content_result_ticket_mismatch",
                    "content service accepted a different world ticket",
                )
                .with_content_ticket(request.ticket)));
            }
            Err(ContentSubmitError::Busy) => {}
            Err(ContentSubmitError::Closed) => {
                self.world_content_result = Some(Err(SessionFailure::new(
                    "session.world_resolver_missing",
                    "world-content service is closed",
                )
                .with_content_ticket(request.ticket)));
            }
            Err(ContentSubmitError::Rejected(error)) => {
                self.world_content_result = Some(Err(error.with_content_ticket(request.ticket)));
            }
        }
    }

    pub(super) fn prepare_world_content(
        &mut self,
        context: WorldContext,
        transition_id: Option<TransitionId>,
        manifest: minimium_world::world::WorldManifest,
        expected_fingerprint: u64,
    ) -> Result<WorldContentProgress, SessionFailure> {
        let existing_ticket = self
            .world_content_request
            .map(|request| request.ticket)
            .or_else(|| {
                self.pending_world_content
                    .as_ref()
                    .filter(|pending| {
                        pending.context == context && Some(pending.transition_id) == transition_id
                    })
                    .map(|pending| pending.ticket)
            });
        let ticket = match existing_ticket.filter(|ticket| {
            ticket.context == context
                && ticket.transition_id == transition_id
                && ticket.content_revision == manifest.content_revision().get()
        }) {
            Some(ticket) => ticket,
            None => {
                let request_id = self.next_world_content_request_id;
                self.next_world_content_request_id =
                    request_id.checked_add(1).ok_or_else(|| {
                        SessionFailure::new(
                            "session.world_content_ticket_exhausted",
                            "world-content request identifiers are exhausted",
                        )
                    })?;
                ContentResolutionTicket {
                    session_id: self.content_jobs.session_id(),
                    request_id,
                    context,
                    transition_id,
                    content_revision: manifest.content_revision().get(),
                }
            }
        };
        if self
            .content_for_resolution_ticket(ticket)
            .is_some_and(|content| {
                content.manifest() == manifest
                    && content.map().fingerprint() == expected_fingerprint
            })
        {
            self.world_content_request = None;
            return Ok(WorldContentProgress::Ready);
        }
        if self
            .world_content_started
            .is_some_and(|started| started.elapsed() >= Duration::from_secs(300))
        {
            self.cancel_world_content_job();
            return Err(SessionFailure::new(
                "session.world_content_timeout",
                "world-content preparation exceeded 300 seconds",
            )
            .with_content_ticket(ticket));
        }
        if self.world_content_request.map(|request| request.ticket) != Some(ticket) {
            self.cancel_world_content_job();
            self.world_content_request = Some(WorldContentRequest {
                ticket,
                manifest,
                expected_fingerprint,
            });
            self.world_content_started.get_or_insert_with(Instant::now);
        }
        self.pump_world_content_job();
        let Some(result) = self.world_content_result.take() else {
            return Ok(WorldContentProgress::Pending);
        };
        self.world_content_job = None;
        let (content, water_catalog) = result
            .map_err(|error| error.with_content_ticket(ticket))?
            .into_parts();
        self.install_resolved_world_content(
            content,
            water_catalog,
            ticket,
            context,
            transition_id,
            manifest,
            expected_fingerprint,
        )
        .map_err(|error| error.with_content_ticket(ticket))?;
        Ok(WorldContentProgress::Ready)
    }
}
