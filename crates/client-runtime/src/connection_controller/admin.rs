use super::*;
use minimium_commands::admin::{AdminPayload, AdminReplyV1, AdminRequestV1, Correlation};
use minimium_commands::operation::{CommandValue, OperationId};
use minimium_commands::results::Stage;
use minimium_commands::{CommandError, CommandErrorCode};

pub(super) struct Pending {
    pub correlation: Correlation,
    session: u64,
    operation: OperationId,
    worker: Option<JoinHandle<AdminReplyV1>>,
    delivery: Arc<admin_client::Delivery>,
}
impl Drop for Pending {
    fn drop(&mut self) {
        self.delivery.cancel_before_send();
    }
}

impl ConnectionController {
    pub(super) fn cancel_unsent_admin(&self) {
        if let Some(p) = &self.admin_request {
            p.delivery.cancel_before_send();
        }
    }
    pub fn admin_available(&self) -> bool {
        self.session_owns_server
            && self
                .session
                .as_ref()
                .is_some_and(|s| s.state() == SessionState::Connected)
    }
    pub fn pending_admin(&self) -> Option<Correlation> {
        self.admin_request.as_ref().map(|p| p.correlation)
    }
    pub(super) fn request_admin_command(&mut self, command: String) -> Result<(), ControllerError> {
        self.submit_admin(AdminPayload::Text { text: command })
            .map(|_| ())
    }

    pub fn request_initial_clock_config(
        &mut self,
        now: ClientTime,
    ) -> Result<Correlation, ControllerError> {
        self.accept_time(now).map_err(ControllerError::from_time)?;
        self.submit_admin(AdminPayload::Operation {
            operation_id: OperationId("world.time.initial_config".into()),
            access: minimium_commands::syntax::canonical::AccessKind::Read,
            arguments: vec![],
            value: None,
        })
    }
    pub fn request_admin_help(
        &mut self,
        target: &str,
        now: ClientTime,
    ) -> Result<Correlation, ControllerError> {
        self.accept_time(now).map_err(ControllerError::from_time)?;
        self.submit_admin(AdminPayload::Operation {
            operation_id: OperationId("ui.console.help".into()),
            access: minimium_commands::syntax::canonical::AccessKind::Invoke,
            arguments: vec![CommandValue::Enum(target.into())],
            value: None,
        })
    }
    fn submit_admin(&mut self, payload: AdminPayload) -> Result<Correlation, ControllerError> {
        self.submit_contextual_admin(payload, false)
    }
    pub(super) fn submit_contextual_admin(
        &mut self,
        payload: AdminPayload,
        needs_observer: bool,
    ) -> Result<Correlation, ControllerError> {
        if !self
            .session
            .as_ref()
            .is_some_and(|session| session.state() == SessionState::Connected)
            || !self.session_owns_server
        {
            return Err(ControllerError::new(
                "client_runtime.admin_control_unavailable",
                "administrative commands require a connection to a server started by this client",
            ));
        }
        if self.admin_request.is_some() {
            return Err(ControllerError::new(
                "client_runtime.admin_request_busy",
                "an administrative request is already in progress",
            ));
        }
        let endpoint = self
            .local_server
            .as_mut()
            .ok_or_else(|| {
                ControllerError::new(
                    "client_runtime.local_server_unavailable",
                    "owned server is unavailable",
                )
            })?
            .owned_admin_endpoint()
            .map_err(|error| {
                ControllerError::caused_by(
                    "client_runtime.admin_endpoint_failed",
                    "could not resolve the owned server admin endpoint",
                    error,
                )
            })?
            .ok_or_else(|| {
                ControllerError::new(
                    "client_runtime.local_server_not_running",
                    "owned server is not running",
                )
            })?;

        let nonce = match self.admin_nonce {
            Some(n) => n,
            None => {
                let n = admin_client::new_nonce().map_err(|e| {
                    ControllerError::caused_by(
                        "client_runtime.admin_nonce_failed",
                        "could not create administrative session",
                        e,
                    )
                })?;
                self.admin_nonce = Some(n);
                n
            }
        };
        let id = self.next_admin_id.ok_or_else(|| {
            ControllerError::new(
                "client_runtime.admin_id_exhausted",
                "administrative request identifiers exhausted",
            )
        })?;
        let correlation = Correlation {
            nonce,
            request_id: id,
        };
        let mut request = AdminRequestV1::new(correlation, payload);
        if needs_observer {
            let (actor, context) = self
                .session
                .as_ref()
                .and_then(|s| s.observer())
                .ok_or_else(|| {
                    ControllerError::new(
                        "context_required",
                        "solar time requires an active observer",
                    )
                })?;
            request.actor = Some(actor.get());
            request.expected_context = Some(minimium_commands::results::WorldBinding {
                instance: context.instance().get(),
                content_revision: context.content_revision().get(),
                membership_epoch: context.membership_epoch().get(),
            });
        }
        request.encode().map_err(|e| {
            ControllerError::new("client_runtime.admin_invalid_request", e.to_string())
        })?;
        let operation = request.operation();
        let session = self
            .session
            .as_ref()
            .expect("checked session")
            .content_jobs_session_id();
        let delivery = Arc::new(admin_client::Delivery::default());
        let gate = delivery.clone();
        let worker = thread::Builder::new()
            .name("minimium-admin".into())
            .spawn(move || admin_client::request_v1_cancellable(&endpoint, &request, &gate))
            .map_err(|e| {
                ControllerError::caused_by(
                    "client_runtime.admin_worker_failed",
                    "could not start administrative worker",
                    e,
                )
            })?;
        self.admin_request = Some(Pending {
            correlation,
            operation,
            session,
            worker: Some(worker),
            delivery,
        });
        self.next_admin_id = id.checked_add(1);
        Ok(correlation)
    }

    pub fn poll_admin_result(
        &mut self,
        now: ClientTime,
    ) -> Result<Option<AdminReplyV1>, ClientTimeError> {
        self.accept_time(now)?;
        let Some(p) = self.admin_request.as_ref() else {
            return Ok(None);
        };
        if !self.admin_available()
            || self.session.as_ref().map(|s| s.content_jobs_session_id()) != Some(p.session)
        {
            p.delivery.cancel_before_send();
        }
        if !p.worker.as_ref().is_some_and(JoinHandle::is_finished) {
            return Ok(None);
        }
        let mut p = self.admin_request.take().expect("pending worker");
        let reply = p
            .worker
            .take()
            .expect("pending worker")
            .join()
            .unwrap_or_else(|_| {
                AdminReplyV1::new(
                    p.correlation,
                    p.correlation.failure(
                        p.operation.clone(),
                        Stage::UnknownAfterDisconnect,
                        "admin_worker",
                        CommandError::new(
                            CommandErrorCode::UnknownAfterDisconnect,
                            Some("admin"),
                            "administrative worker stopped before returning a reply",
                            None::<String>,
                        ),
                    ),
                )
            });
        Ok(Some(reply))
    }
}
