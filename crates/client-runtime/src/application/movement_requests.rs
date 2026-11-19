use super::*;
use crate::{ClientTime, ConnectionController};
use minimium_commands::results::{OperationResult, Stage};
use minimium_protocol::MovementPatch;
impl ClientApplicationCore {
    pub fn request_jump(&mut self, now: ClientTime) -> OperationResult {
        if let Err(error) = self.accept_time(now) {
            return self
                .connection
                .movement_time_failure("player.movement.jump", error);
        }
        self.connection.request_jump(now)
    }
    pub fn request_movement_policy(
        &mut self,
        patch: MovementPatch,
        expected_revision: Option<u64>,
        now: ClientTime,
    ) -> OperationResult {
        if let Err(error) = self.accept_time(now) {
            return self
                .connection
                .movement_time_failure(crate::connection_controller::patch_path(patch), error);
        }
        self.connection
            .request_movement_policy(patch, expected_revision, now)
    }
}

pub(super) fn queue_policy(
    connection: &mut ConnectionController,
    patch: MovementPatch,
    now: ClientTime,
) -> LocalActionOutcome {
    let result = connection.request_movement_policy(patch, None, now);
    let kind = QueuedActionKind::Other;
    match result.receipt.stage {
        Stage::Submitted => LocalActionOutcome::Queued {
            kind,
            movement: None,
        },
        Stage::Suppressed => LocalActionOutcome::Suppressed(
            if result.outcome.err().is_some_and(|e| e.code == "paused") {
                SuppressionReason::Paused
            } else {
                SuppressionReason::SessionNotConnected
            },
        ),
        _ => LocalActionOutcome::Rejected {
            kind,
            error: match result.outcome.err().as_ref().map(|e| e.code.as_str()) {
                Some("busy") => crate::QueueError::BufferFull,
                Some("invalid_value") => {
                    crate::QueueError::Protocol(minimium_protocol::ProtocolError::InvalidPayload)
                }
                _ => crate::QueueError::Disconnected,
            },
        },
    }
}
