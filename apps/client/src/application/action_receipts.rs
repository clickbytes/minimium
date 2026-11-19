use super::{ClientApplication, LocalActionOutcome, SuppressionReason};
use crate::client_commands::{
    output::{self, ClientResult},
    player_actions::{PlayerIntent, PlayerReceipt},
    CommandSource,
};
use minimium_client_runtime::{ClientStepOutput, ClientTime, QueueError, QueuedWorldFrame};
use minimium_commands::{
    results::{OperationContext, Stage, WorldBinding},
    CommandError, CommandErrorCode as Code,
};
use std::time::Duration;

struct Pending {
    identity: OperationContext,
    source: CommandSource,
    frame: QueuedWorldFrame,
    receipt: PlayerReceipt,
    submitted: ClientTime,
    sent: bool,
}

#[derive(Default)]
pub(super) struct PendingActions(Vec<Pending>);
const MAX_PENDING: usize = 64;
const OBSERVATION_WINDOW: Duration = Duration::from_secs(30);

fn fail(id: OperationContext, stage: Stage, code: Code, reason: &str) -> ClientResult {
    let subject = id.operation.0.clone();
    output::failure(
        id,
        stage,
        "player",
        CommandError::new(code, Some(subject), reason, None::<String>),
    )
}

fn binding(context: minimium_protocol::WorldContext) -> WorldBinding {
    WorldBinding {
        instance: context.instance().get(),
        content_revision: context.content_revision().get(),
        membership_epoch: context.membership_epoch().get(),
    }
}

impl ClientApplication {
    pub(crate) fn player_operation(
        &mut self,
        id: OperationContext,
        intent: &PlayerIntent,
        source: CommandSource,
        now: ClientTime,
    ) -> ClientResult {
        let Some(session) = self.session() else {
            return fail(
                id,
                Stage::Suppressed,
                Code::NoSession,
                "session unavailable",
            );
        };
        let Some(context) = session.active_world_context() else {
            return fail(
                id,
                Stage::Suppressed,
                Code::Unavailable,
                "world unavailable",
            );
        };
        if id.session != Some(session.content_jobs_session_id())
            || id.context.as_ref() != Some(&binding(context))
        {
            return fail(
                id,
                Stage::Rejected,
                Code::StaleContext,
                "session or world changed before submission",
            );
        }
        let (action, aim) = intent.prepare(source);
        if aim.is_some_and(|(dir, _)| !minimium_protocol::is_unit_direction(dir)) {
            return fail(
                id,
                Stage::Rejected,
                Code::InvalidValue,
                "aim direction must be a unit vector",
            );
        }
        let before = session.metrics().last_queued_world_frame;
        match self.core.execute_action(action, now) {
            LocalActionOutcome::Suppressed(reason) => {
                return fail(
                    id,
                    Stage::Suppressed,
                    match reason {
                        SuppressionReason::NoSession => Code::NoSession,
                        _ => Code::Unavailable,
                    },
                    &format!("{reason:?}"),
                )
            }
            LocalActionOutcome::Rejected { error, .. } => {
                return fail(
                    id,
                    Stage::Rejected,
                    match error {
                        QueueError::BufferFull => Code::QueueFull,
                        QueueError::StaleWorldContext => Code::StaleContext,
                        QueueError::Disconnected => Code::NoSession,
                        _ => Code::Unavailable,
                    },
                    &error.to_string(),
                )
            }
            LocalActionOutcome::Queued { .. } => {}
        }
        let frame = self
            .session()
            .and_then(|s| s.metrics().last_queued_world_frame)
            .filter(|f| Some(*f) != before)
            .expect("player executor accepted a world frame");
        let receipt = PlayerReceipt {
            sequence: frame.sequence,
            aim,
        };
        let result = receipt.result(id.clone(), Stage::Submitted);
        self.action_receipts.0.push(Pending {
            identity: id,
            source,
            frame,
            receipt,
            submitted: now,
            sent: false,
        });
        result
    }

    /// The caller delivers an evicted receipt before returning the newer submission.
    pub(crate) fn retire_player_receipt(&mut self) -> Option<(CommandSource, ClientResult)> {
        if self.action_receipts.0.len() <= MAX_PENDING {
            return None;
        }
        let p = self.action_receipts.0.remove(0);
        Some((
            p.source,
            fail(
                p.identity,
                Stage::Partial,
                Code::ResultLimit,
                "observation window replaced; owner outcome unknown",
            ),
        ))
    }

    pub(crate) fn player_receipts(
        &mut self,
        step: &ClientStepOutput,
        now: ClientTime,
    ) -> Vec<(CommandSource, ClientResult)> {
        let mut outputs = Vec::new();
        let current = self.core.session();
        for applied in &step.session_events {
            let minimium_client_runtime::SessionEvent::Message(
                minimium_protocol::ServerMessage::PlayerActionReceipt(receipt),
            ) = &applied.event
            else {
                continue;
            };
            let event_session = current
                .or_else(|| step.disconnect.as_ref().and_then(|d| d.session.as_ref()))
                .map(|s| s.content_jobs_session_id());
            let Some(index) = self.action_receipts.0.iter().position(|p| {
                p.identity.session == event_session
                    && p.receipt.sequence == receipt.sequence
                    && p.identity.context.as_ref() == Some(&binding(receipt.context))
            }) else {
                continue;
            };
            let pending = &self.action_receipts.0[index];
            let expected = match pending.identity.operation.0.as_str() {
                "player.movement.move" => minimium_protocol::PlayerActionKind::Move,
                "player.chat.send" => minimium_protocol::PlayerActionKind::Chat,
                "player.identity.set_name" => minimium_protocol::PlayerActionKind::Name,
                "player.combat.fire" => minimium_protocol::PlayerActionKind::Fire,
                "player.inventory.grant_tools" => minimium_protocol::PlayerActionKind::GrantTools,
                _ => continue,
            };
            if receipt.kind != expected {
                continue;
            }
            let stage = match receipt.decision {
                minimium_protocol::PlayerActionDecision::Accepted => Stage::OwnerAccepted,
                minimium_protocol::PlayerActionDecision::Committed => Stage::OwnerCommitted,
                minimium_protocol::PlayerActionDecision::Rejected(_) => Stage::Rejected,
            };
            let result = match receipt.decision {
                minimium_protocol::PlayerActionDecision::Rejected(reason) => fail(
                    pending.identity.clone(),
                    stage,
                    match reason {
                        1 => Code::PermissionDenied,
                        2 => Code::Busy,
                        3 => Code::InvalidValue,
                        4 => Code::Busy,
                        6 => Code::StaleContext,
                        8 => Code::Conflict,
                        _ => Code::Unavailable,
                    },
                    &format!("owner rejection {reason}"),
                ),
                _ => pending.receipt.result(pending.identity.clone(), stage),
            };
            outputs.push((pending.source, result));
            if stage != Stage::OwnerAccepted {
                self.action_receipts.0.remove(index);
            }
        }
        self.action_receipts.0.retain_mut(|p| {
            let same = current.filter(|s| p.identity.session == Some(s.content_jobs_session_id()));
            let retired = step
                .disconnect
                .as_ref()
                .and_then(|d| d.session.as_ref())
                .filter(|s| p.identity.session == Some(s.content_jobs_session_id()));
            if !p.sent
                && same
                    .or(retired)
                    .is_some_and(|s| s.world_frame_sent(p.frame) == Some(true))
            {
                p.sent = true;
                outputs.push((p.source, p.receipt.result(p.identity.clone(), Stage::Sent)));
            }
            let end = if same.is_none() {
                Some((
                    Stage::UnknownAfterDisconnect,
                    Code::UnknownAfterDisconnect,
                    "session ended without a correlated owner reply",
                ))
            } else if same.and_then(|s| s.active_world_context()).map(binding) != p.identity.context
            {
                Some((
                    Stage::Partial,
                    Code::StaleContext,
                    "world changed; owner outcome unknown",
                ))
            } else if now
                .as_instant()
                .saturating_duration_since(p.submitted.as_instant())
                >= OBSERVATION_WINDOW
            {
                Some((
                    Stage::Partial,
                    Code::Timeout,
                    "receipt observation expired; owner outcome unknown",
                ))
            } else {
                None
            };
            if let Some((stage, code, reason)) = end {
                outputs.push((p.source, fail(p.identity.clone(), stage, code, reason)));
                false
            } else {
                true
            }
        });
        outputs
    }

    pub(super) fn end_player_requests(&mut self) -> Vec<(CommandSource, ClientResult)> {
        self.action_receipts
            .0
            .drain(..)
            .map(|p| {
                (
                    p.source,
                    fail(
                        p.identity,
                        Stage::UnknownAfterDisconnect,
                        Code::UnknownAfterDisconnect,
                        "shutdown before owner confirmation",
                    ),
                )
            })
            .collect()
    }
}
