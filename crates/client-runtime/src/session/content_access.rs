use super::*;

impl ClientSession {
    pub fn active_world_context(&self) -> Option<WorldContext> {
        self.world_context
    }

    pub fn shares_active_world_state(&self, context: WorldContext) -> bool {
        let Some(active_context) = self.world_context else {
            return false;
        };
        if active_context == context {
            return true;
        }
        let Some(active) = self.active_world_content.as_ref() else {
            return false;
        };
        let Some(pending) = self.pending_world_content.as_ref() else {
            return false;
        };
        active_context.instance() == context.instance()
            && pending.context == context
            && active.context == active_context
            && active.content.manifest() == pending.content.manifest()
            && active.content.map().fingerprint() == pending.content.map().fingerprint()
    }

    pub fn prepared_geography(&self) -> Option<Arc<dyn minimium_procgen::PreparedGeography>> {
        self.active_world_content
            .as_ref()
            .and_then(|bundle| bundle.content.geography().cloned())
    }

    pub fn resolved_relief_policy(&self) -> Option<minimium_procgen::ResolvedPlanetReliefPolicy> {
        self.active_world_content
            .as_ref()
            .filter(|bundle| Some(bundle.context) == self.world_context)
            .and_then(|bundle| bundle.content.resolved_relief_policy())
    }

    pub(crate) fn set_world_content_resolver(
        &mut self,
        session_id: u64,
        resolver: Option<Arc<dyn WorldContentResolver>>,
    ) {
        self.session_id = session_id;
        self.world_content_resolver = resolver;
    }

    pub fn resolved_world_content_for_context(
        &self,
        context: WorldContext,
    ) -> Option<Arc<ResolvedWorldContent>> {
        self.active_world_content
            .as_ref()
            .filter(|bundle| bundle.context == context)
            .map(|bundle| bundle.content.clone())
            .or_else(|| {
                self.pending_world_content
                    .as_ref()
                    .filter(|bundle| bundle.context == context)
                    .map(|bundle| bundle.content.clone())
            })
    }

    pub fn pending_geography(
        &self,
        context: WorldContext,
        transition_id: TransitionId,
    ) -> Option<Arc<dyn minimium_procgen::PreparedGeography>> {
        self.pending_world_content
            .as_ref()
            .filter(|bundle| bundle.context == context && bundle.transition_id == transition_id)
            .and_then(|bundle| bundle.content.geography().cloned())
    }

    pub(crate) fn pending_content_for_transition(
        &self,
        transition_id: TransitionId,
        context: WorldContext,
    ) -> Option<Arc<ResolvedWorldContent>> {
        self.pending_world_content
            .as_ref()
            .filter(|bundle| bundle.transition_id == transition_id && bundle.context == context)
            .map(|bundle| bundle.content.clone())
    }

    pub(crate) fn content_for_resolution_ticket(
        &self,
        ticket: ContentResolutionTicket,
    ) -> Option<Arc<ResolvedWorldContent>> {
        if let Some(transition_id) = ticket.transition_id {
            return self
                .pending_world_content
                .as_ref()
                .filter(|pending| {
                    pending.context == ticket.context
                        && pending.transition_id == transition_id
                        && pending.ticket == ticket
                })
                .map(|pending| pending.content.clone());
        }
        self.active_world_content
            .as_ref()
            .filter(|active| active.context == ticket.context && active.content.ticket() == ticket)
            .map(|active| active.content.clone())
    }

    pub fn cancel_pending_transition(&mut self) -> bool {
        let Some((transition_id, _)) = self.pending_transition else {
            return false;
        };
        if self.cancel_requested_transition == Some(transition_id) {
            return true;
        }
        if self
            .queue(&ClientMessage::CancelTransition(transition_id))
            .is_err()
        {
            return false;
        }
        self.cancel_requested_transition = Some(transition_id);
        true
    }

    /// Records host readiness; the session queues the protocol message on poll.
    pub fn report_transfer_host_ready(&mut self, ticket: TransferTicket) -> Result<(), QueueError> {
        let transition = (ticket.transition_id(), ticket.context());
        if self.pending_transition != Some(transition)
            || self.cancel_requested_transition == Some(ticket.transition_id())
            || self
                .pending_content_for_transition(ticket.transition_id(), ticket.context())
                .is_none()
        {
            return Err(QueueError::StaleWorldContext);
        }
        self.host_ready_transition = Some(transition);
        Ok(())
    }

    /// Returns true after host preparation was reported or cancellation closed readiness.
    pub fn host_readiness_requested(&self, ticket: TransferTicket) -> bool {
        self.has_host_readiness_request(ticket)
    }
}
