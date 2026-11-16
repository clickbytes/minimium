use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PreparedSessionError {
    Time(ClientTimeError),
    Busy,
    InvalidState,
    ForeignContentService,
    ReusedContentServiceId,
    ReplicaSchema(crate::ReplicaError),
}

impl ConnectionController {
    /// Installs a host-prepared in-memory session into this controller's one
    /// session slot. It cannot replace an active attempt/session or assume
    /// local-server ownership.
    pub(crate) fn install_prepared_session(
        &mut self,
        mut session: ClientSession,
        now: ClientTime,
    ) -> Result<(), PreparedSessionError> {
        self.validate_time(now)
            .map_err(PreparedSessionError::Time)?;
        session
            .validate_time(now)
            .map_err(PreparedSessionError::Time)?;
        if self.busy() || self.connector.has_active_worker() {
            return Err(PreparedSessionError::Busy);
        }
        if session.state() != SessionState::AwaitingWelcome
            || session.active_world_context().is_some()
        {
            return Err(PreparedSessionError::InvalidState);
        }
        let session_id = session.content_jobs_session_id();
        if session_id == 0 {
            return Err(PreparedSessionError::ReusedContentServiceId);
        }
        if !self.connector.can_reserve_attempt_ids_through(session_id) {
            if self.connector.has_active_worker() {
                return Err(PreparedSessionError::Busy);
            }
            return Err(PreparedSessionError::ReusedContentServiceId);
        }
        let session_port = session.content_jobs_port();
        if self
            .content_jobs
            .as_ref()
            .is_some_and(|port| !Arc::ptr_eq(port, &session_port))
        {
            return Err(PreparedSessionError::ForeignContentService);
        }
        for (kind, schema) in &self.replica_schemas {
            session
                .register_replica_schema(*kind, *schema)
                .map_err(PreparedSessionError::ReplicaSchema)?;
        }
        session.set_diagnostic_logger(self.diagnostic_logger.clone());
        if !self.connector.reserve_attempt_ids_through(session_id) {
            return Err(PreparedSessionError::Busy);
        }
        self.accept_time(now).map_err(PreparedSessionError::Time)?;
        self.content_jobs.get_or_insert(session_port);
        self.content_jobs_shutdown = false;
        self.session = Some(session);
        self.session_owns_server = false;
        Ok(())
    }
}
