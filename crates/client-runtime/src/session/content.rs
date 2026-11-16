//! Logical material preparation and its five-minute deadline.
use super::*;
use crate::{
    ContentJobPoll, ContentJobRequest, ContentJobResult, ContentJobTicket, ContentSubmitError,
    MaterialContentRequest, MaterialContentTicket,
};
use minimium_asset_io::materials::MaterialPackage;
use minimium_environment::climate::ClimateScenario;

#[derive(Default)]
pub(super) struct ContentPreparation {
    pub capability: Option<bool>,
    pub started: Option<ClientTime>,
    pub hash: [u8; 32],
    pub climate: Arc<ClimateScenario>,
    pub awaiting_gpu: bool,
    pub package: Option<MaterialPackage>,
    request: Option<MaterialContentRequest>,
    job: Option<ContentJobHandle>,
}

impl ClientSession {
    pub fn climate_scenario(&self) -> Arc<ClimateScenario> {
        self.content.climate.clone()
    }
    pub fn content_awaiting_gpu(&self) -> bool {
        self.content.awaiting_gpu
    }
    pub fn take_material_package(&mut self) -> Option<MaterialPackage> {
        self.content.package.take()
    }

    /// The presentation host acknowledges only after successful GPU installation.
    pub fn finish_content(&mut self, now: ClientTime) -> Result<(), QueueError> {
        self.accept_time(now).map_err(QueueError::Time)?;
        if !self.content.awaiting_gpu {
            return Err(QueueError::Protocol(ProtocolError::InvalidPayload));
        }
        self.append_encoded(&ClientMessage::ContentReady(self.content.hash))
            .map_err(QueueError::Protocol)?;
        self.content.awaiting_gpu = false;
        self.content.started = None;
        self.content.request = None;
        self.state_since = self.now;
        Ok(())
    }

    pub(super) fn cancel_content_jobs(&mut self) {
        self.cancel_world_content_job();
        if let Some(handle) = self.content.job.as_ref() {
            handle.request_cancel();
            self.content_jobs.port().cancel(handle);
        }
    }

    pub(super) fn begin_content(
        &mut self,
        hash: [u8; 32],
        port: u16,
        climate: ClimateScenario,
    ) -> Result<(), SessionFailure> {
        if self.state != SessionState::AwaitingWelcome
            || self.content.started.is_some()
            || self.content.capability.is_none()
        {
            return Err(SessionFailure::new(
                "session.unexpected_content_offer",
                "unexpected content offer",
            ));
        }
        climate
            .validate()
            .map_err(|error| SessionFailure::new("session.invalid_climate_scenario", error))?;
        self.content.climate = Arc::new(climate);
        self.content.started = Some(self.now);
        self.content.hash = hash;
        if hash == [0; 32] {
            self.content.awaiting_gpu = true;
            return Ok(());
        }
        if port == 0 {
            return Err(SessionFailure::new(
                "session.material_endpoint_missing",
                "required material package has no HTTP endpoint",
            ));
        }
        let mut endpoint = self
            .transport
            .as_ref()
            .ok_or_else(|| {
                SessionFailure::new("session.game_socket_missing", "missing game socket")
            })?
            .peer_addr()
            .map_err(|error| {
                SessionFailure::caused_by(
                    "session.peer_address_failed",
                    "could not read connected game peer address",
                    error,
                )
            })?;
        endpoint.set_port(port);
        let request_id = self.next_material_content_request_id;
        self.next_material_content_request_id = request_id.checked_add(1).ok_or_else(|| {
            SessionFailure::new(
                "session.material_content_ticket_exhausted",
                "material-content request identifiers are exhausted",
            )
        })?;
        let ticket = MaterialContentTicket {
            session_id: self.content_jobs.session_id(),
            request_id,
            manifest_hash: hash,
        };
        self.content.request = Some(MaterialContentRequest {
            ticket,
            endpoint,
            compressed: self.content.capability.unwrap(),
        });
        self.poll_content()
    }

    pub(super) fn poll_content(&mut self) -> Result<(), SessionFailure> {
        if self
            .content
            .started
            .is_some_and(|start| self.now.elapsed_since(start) >= Duration::from_secs(300))
        {
            let mut error = SessionFailure::new(
                "session.content_preparation_timeout",
                "material preparation deadline exceeded; reconnect to retry",
            );
            if let Some(request) = self.content.request {
                error = error.with_material_ticket(request.ticket);
            }
            return Err(error);
        }
        if self.content.awaiting_gpu {
            return Ok(());
        }
        let Some(request) = self.content.request else {
            return Ok(());
        };
        if self.content.job.is_none() {
            match self
                .content_jobs
                .port()
                .submit(ContentJobRequest::Material(request))
            {
                Ok(handle) if handle.ticket() == ContentJobTicket::Material(request.ticket) => {
                    self.content.job = Some(handle);
                }
                Ok(handle) => {
                    handle.request_cancel();
                    self.content_jobs.port().cancel(&handle);
                    return Err(SessionFailure::new(
                        "session.material_result_ticket_mismatch",
                        "content service accepted a different material ticket",
                    )
                    .with_material_ticket(request.ticket));
                }
                Err(ContentSubmitError::Busy) => return Ok(()),
                Err(ContentSubmitError::Closed) => {
                    return Err(SessionFailure::new(
                        "session.material_worker_failed",
                        "content service is closed",
                    )
                    .with_material_ticket(request.ticket))
                }
                Err(ContentSubmitError::Rejected(error)) => {
                    return Err(error.with_material_ticket(request.ticket))
                }
            }
            // As before, starting work does not install its result in the same
            // material-offer dispatch. A later poll observes completion.
            return Ok(());
        }
        let handle = self.content.job.as_ref().unwrap();
        let completion = match self.content_jobs.port().poll(handle.ticket()) {
            ContentJobPoll::Pending => return Ok(()),
            ContentJobPoll::Complete(completion) if completion.ticket == handle.ticket() => {
                completion
            }
            ContentJobPoll::Complete(_) => return Ok(()),
            ContentJobPoll::Cancelled | ContentJobPoll::Unknown => {
                return Err(SessionFailure::new(
                    "session.material_worker_failed",
                    "material work stopped unexpectedly",
                )
                .with_material_ticket(request.ticket));
            }
        };
        if handle.is_cancelled() {
            return Ok(());
        }
        let package = match completion.result {
            ContentJobResult::Material(result) => {
                result.map_err(|error| error.with_material_ticket(request.ticket))?
            }
            ContentJobResult::World(_) => {
                return Err(SessionFailure::new(
                    "session.material_result_kind_mismatch",
                    "content service returned a world result for material preparation",
                )
                .with_material_ticket(request.ticket))
            }
        };
        let expected: String = request
            .ticket
            .manifest_hash
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if package.manifest_hash != expected || package.variant != usize::from(!request.compressed)
        {
            return Err(SessionFailure::new(
                "session.material_identity_mismatch",
                "prepared material package does not match the offered hash and variant",
            )
            .with_material_ticket(request.ticket));
        }
        self.content.package = Some(package);
        self.content.awaiting_gpu = true;
        self.content.job = None;
        Ok(())
    }
}
