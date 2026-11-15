use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PrepareIdentity {
    pub(super) id: TransitionId,
    pub(super) source: WorldContext,
    pub(super) target: WorldContext,
    pub(super) map_fingerprint: u64,
    pub(super) destination: [f32; 3],
    pub(super) destination_mm: minimium_world::world::Position3Mm,
    pub(super) manifest: minimium_world::world::WorldManifest,
    pub(super) clock_sample: ClockSample,
    pub(super) environment: EnvironmentDescriptor,
}

impl ClientSession {
    pub(super) fn validate_prepare_identity(&self, identity: PrepareIdentity) -> bool {
        if self
            .highest_terminated_transition_id
            .is_some_and(|id| identity.id <= id)
        {
            return false;
        }
        self.pending_prepare_identity
            .is_none_or(|current| current == identity)
    }

    pub(super) fn install_prepare_identity(&mut self, identity: PrepareIdentity) {
        self.pending_prepare_identity = Some(identity);
    }

    pub(super) fn validate_prepare_message(&self, message: &ServerMessage) -> bool {
        let ServerMessage::TransitionPrepare {
            transition_id,
            source_context,
            target_context,
            map_fingerprint,
            destination,
            destination_mm,
            manifest,
            clock_sample,
            environment,
        } = message
        else {
            return false;
        };
        self.validate_prepare_identity(PrepareIdentity {
            id: *transition_id,
            source: *source_context,
            target: *target_context,
            map_fingerprint: *map_fingerprint,
            destination: *destination,
            destination_mm: *destination_mm,
            manifest: *manifest,
            clock_sample: *clock_sample,
            environment: *environment,
        })
    }

    pub(super) fn has_host_readiness_request(&self, ticket: TransferTicket) -> bool {
        self.host_ready_transition == Some((ticket.transition_id(), ticket.context()))
            || (self.pending_transition == Some((ticket.transition_id(), ticket.context()))
                && self.cancel_requested_transition == Some(ticket.transition_id()))
    }

    pub(super) fn clear_transfer_lifecycle(&mut self) {
        if let Some((transition_id, _)) = self.pending_transition {
            self.mark_transition_terminated(transition_id);
        }
        self.pending_transition = None;
        self.ready_transition = None;
        self.cancel_requested_transition = None;
        self.host_ready_transition = None;
        self.pending_prepare_identity = None;
    }

    pub(super) fn mark_transition_terminated(&mut self, id: TransitionId) {
        self.highest_terminated_transition_id = Some(
            self.highest_terminated_transition_id
                .map_or(id, |current| current.max(id)),
        );
    }

    pub(super) fn pump_transition_readiness(&mut self) {
        let Some(transition) = self.host_ready_transition else {
            return;
        };
        if self.pending_transition != Some(transition)
            || self.cancel_requested_transition == Some(transition.0)
            || self.ready_transition == Some(transition)
            || self
                .pending_content_for_transition(transition.0, transition.1)
                .is_none()
        {
            return;
        }
        if self
            .queue(&ClientMessage::TransitionReady {
                transition_id: transition.0,
                target_context: transition.1,
            })
            .is_ok()
        {
            self.ready_transition = Some(transition);
        }
    }
}
