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
    candidate_transfer: Option<minimium_client_runtime::TransferTicket>,
    transfer: Option<minimium_client_runtime::TransferTicket>,
    ready_reported: bool,
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
            navigation: intent.navigation_detail(),
        };
        let result = receipt.result(id.clone(), Stage::Submitted);
        self.action_receipts.0.push(Pending {
            identity: id,
            source,
            frame,
            receipt,
            submitted: now,
            sent: false,
            candidate_transfer: None,
            transfer: None,
            ready_reported: false,
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
            if let minimium_client_runtime::SessionEvent::Message(
                minimium_protocol::ServerMessage::TransitionPrepare {
                    transition_id,
                    source_context,
                    target_context,
                    ..
                },
            ) = &applied.event
            {
                let event_session = current
                    .or_else(|| step.disconnect.as_ref().and_then(|d| d.session.as_ref()))
                    .map(|s| s.content_jobs_session_id());
                if let Some(pending) = self.action_receipts.0.iter_mut().find(|p| {
                    p.identity.operation.0 == "player.interaction.portal"
                        && p.identity.session == event_session
                        && p.identity.context.as_ref() == Some(&binding(*source_context))
                        && p.candidate_transfer.is_none()
                        && applied.update.is_ok()
                }) {
                    pending.candidate_transfer =
                        Some(minimium_client_runtime::TransferTicket::new(
                            *transition_id,
                            *target_context,
                        ));
                }
            }
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
            let expected = match self.action_receipts.0[index].identity.operation.0.as_str() {
                "player.movement.move" => minimium_protocol::PlayerActionKind::Move,
                "player.chat.send" => minimium_protocol::PlayerActionKind::Chat,
                "player.identity.set_name" => minimium_protocol::PlayerActionKind::Name,
                "player.combat.fire" => minimium_protocol::PlayerActionKind::Fire,
                "player.inventory.grant_tools" => minimium_protocol::PlayerActionKind::GrantTools,
                "player.interaction.portal" => minimium_protocol::PlayerActionKind::Portal,
                "player.movement.goto" => minimium_protocol::PlayerActionKind::Goto,
                "player.movement.set_altitude" => minimium_protocol::PlayerActionKind::Altitude,
                _ => continue,
            };
            if receipt.kind != expected {
                continue;
            }
            if let minimium_protocol::PlayerActionDecision::Preparing(transition_id) =
                receipt.decision
            {
                let candidate = self.action_receipts.0[index].candidate_transfer;
                if candidate.is_none_or(|ticket| ticket.transition_id() != transition_id) {
                    continue;
                }
                self.action_receipts.0[index].transfer = candidate;
            }
            let pending = &self.action_receipts.0[index];
            let stage = match receipt.decision {
                minimium_protocol::PlayerActionDecision::Accepted => Stage::OwnerAccepted,
                minimium_protocol::PlayerActionDecision::Preparing(_) => Stage::Preparing,
                minimium_protocol::PlayerActionDecision::Committed => Stage::OwnerCommitted,
                minimium_protocol::PlayerActionDecision::Rejected(8)
                    if receipt.kind == minimium_protocol::PlayerActionKind::Portal =>
                {
                    Stage::Cancelled
                }
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
                        9 => Code::UnsupportedContext,
                        10 => Code::Unavailable,
                        11 => Code::Busy,
                        _ => Code::Unavailable,
                    },
                    &format!(
                        "owner rejection: {}",
                        match reason {
                            1 => "permission denied",
                            2 => "busy",
                            3 => "invalid intent",
                            4 => "rate limit",
                            5 => "unavailable",
                            6 => "stale world context",
                            7 => "paused",
                            8 => "superseded or cancelled",
                            9 => "Earth world required",
                            10 => "flight required",
                            11 => "transfer pending",
                            _ => "unavailable",
                        }
                    ),
                ),
                _ => pending.receipt.result(pending.identity.clone(), stage),
            };
            outputs.push((pending.source, result));
            if !matches!(stage, Stage::OwnerAccepted | Stage::Preparing) {
                self.action_receipts.0.remove(index);
            }
        }
        self.action_receipts.0.retain_mut(|p| {
            let same = current.filter(|s| p.identity.session == Some(s.content_jobs_session_id()));
            if !p.ready_reported
                && p.transfer
                    .is_some_and(|ticket| same.is_some_and(|s| s.transition_ready_queued(ticket)))
            {
                p.ready_reported = true;
                outputs.push((p.source, p.receipt.result(p.identity.clone(), Stage::Ready)));
            }
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
