use super::*;
use minimium_commands::operation::{CommandValue as V, FiniteValue, OperationId};
use minimium_commands::results::{
    property::{Freshness, ReadSource},
    OperationContext, OperationFailure, OperationReceipt, OperationResult, Payload, Stage,
    WorldBinding,
};
use minimium_commands::{PropertyLifecycle, PropertySource, PropertyValue};
use minimium_protocol::{MovementPatch, PolicyRejection, PolicyReply, ServerMessage, WorldContext};
use minimium_simulation::MovementPolicy;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MovementField {
    Speed,
    Acceleration,
    Flight,
}
impl MovementField {
    pub fn path(self) -> &'static str {
        match self {
            Self::Speed => "player.movement.speed",
            Self::Acceleration => "player.movement.accel",
            Self::Flight => "player.movement.fly",
        }
    }
    fn value(self, policy: MovementPolicy) -> V {
        match self {
            Self::Speed => number(policy.speed),
            Self::Acceleration => number(policy.accel),
            Self::Flight => V::Bool(policy.can_fly),
        }
    }
}
fn number(v: f32) -> V {
    V::Finite(FiniteValue::new(f64::from(v)).expect("validated policy"))
}
fn path(patch: MovementPatch) -> &'static str {
    match patch {
        MovementPatch::Speed(_) => MovementField::Speed.path(),
        MovementPatch::Acceleration(_) => MovementField::Acceleration.path(),
        MovementPatch::FlightEnabled(_) => MovementField::Flight.path(),
        MovementPatch::SetBoth { .. } => "player.movement.policy",
    }
}
fn binding(context: WorldContext) -> WorldBinding {
    WorldBinding {
        instance: context.instance().get(),
        content_revision: context.content_revision().get(),
        membership_epoch: context.membership_epoch().get(),
    }
}
fn failure(identity: OperationContext, stage: Stage, code: &str, reason: &str) -> OperationResult {
    let subject = Some(identity.operation.0.clone());
    OperationResult::new(
        identity,
        OperationReceipt::at(stage),
        Err(OperationFailure {
            code: code.into(),
            reason: reason.into(),
            subject,
            usage: None,
            span: None,
            origin: "movement_policy".into(),
        }),
    )
}
#[derive(Default)]
pub(super) struct Requests {
    pending: BTreeMap<u64, (OperationContext, MovementPatch)>,
    ready: Vec<OperationResult>,
    context: Option<WorldContext>,
    sampled: Option<(ClientTime, MovementPolicy)>,
}
impl Requests {
    pub(super) fn end_session(&mut self) {
        for (_, (identity, _)) in std::mem::take(&mut self.pending) {
            self.ready.push(failure(
                identity,
                Stage::UnknownAfterDisconnect,
                "unknown_after_disconnect",
                "connection ended before policy acknowledgement",
            ));
        }
        self.context = None;
        self.sampled = None;
    }
}
impl ConnectionController {
    pub(crate) fn policy_time_failure(
        &self,
        patch: MovementPatch,
        error: ClientTimeError,
    ) -> OperationResult {
        failure(
            self.policy_identity(
                self.session
                    .as_ref()
                    .map_or(1, ClientSession::next_world_request_id),
                path(patch),
            ),
            Stage::Rejected,
            "invalid_time",
            &error.to_string(),
        )
    }
    fn policy_identity(&self, request_id: u64, path: &str) -> OperationContext {
        let s = self.session.as_ref();
        OperationContext {
            request_id,
            correlation_nonce: None,
            operation: OperationId(path.into()),
            scope: "SessionActor".into(),
            session: s.map(ClientSession::content_jobs_session_id),
            context: s.and_then(ClientSession::active_world_context).map(binding),
        }
    }

    pub fn movement_property(
        &self,
        field: MovementField,
        request_id: u64,
        now: ClientTime,
    ) -> OperationResult {
        let identity = self.policy_identity(request_id, field.path());
        let Some((sampled, confirmed)) = self.policy.sampled.filter(|_| {
            self.policy.context.is_some()
                && self
                    .session
                    .as_ref()
                    .and_then(ClientSession::active_world_context)
                    == self.policy.context
        }) else {
            return failure(
                identity,
                Stage::Rejected,
                "unavailable",
                "no confirmed policy in the active world",
            );
        };
        let value = field.value(confirmed);
        let pending = self.policy.pending.values().rev().find_map(|(id, patch)| {
            if id.context != identity.context {
                return None;
            }
            (path(*patch) == field.path()
                || matches!(patch, MovementPatch::SetBoth { .. }) && field != MovementField::Flight)
                .then(|| patch.apply(confirmed).map(|p| field.value(p)))
                .flatten()
        });
        let age = now
            .as_instant()
            .saturating_duration_since(sampled.as_instant())
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX);
        let reason = if field == MovementField::Flight {
            Some(format!(
                "session sample; local flight enabled={}",
                self.flight_enabled
            ))
        } else {
            pending
                .is_some()
                .then(|| "awaiting owner acknowledgement".into())
        };
        let property = PropertyValue {
            requested: pending.or_else(|| Some(value.clone())),
            effective: Some(value),
            default: Some(field.value(MovementPolicy::default())),
            lifecycle: PropertyLifecycle::Live,
            startup: None,
            session_override: None,
            revision: Some(confirmed.revision),
            source: PropertySource::Replica,
            read_source: ReadSource::Replica,
            freshness: Freshness::Sampled,
            difference_reason: reason,
        };
        let mut receipt = OperationReceipt::at(Stage::Completed);
        receipt.revision = property.revision;
        receipt.sample_age_ms = Some(age);
        receipt.freshness = Some(Freshness::Sampled);
        OperationResult::new(identity, receipt, Ok(Payload::Property(property)))
    }

    pub fn request_movement_policy(
        &mut self,
        patch: MovementPatch,
        expected_revision: Option<u64>,
        now: ClientTime,
    ) -> OperationResult {
        let id = self
            .session
            .as_ref()
            .map_or(1, ClientSession::next_world_request_id);
        let identity = self.policy_identity(id, path(patch));
        if self.accept_time(now).is_err() {
            return failure(
                identity,
                Stage::Rejected,
                "invalid_time",
                "monotonic time moved backwards",
            );
        }
        let Some(session) = self
            .session
            .as_mut()
            .filter(|s| s.state() == SessionState::Connected)
        else {
            return failure(
                identity,
                Stage::Suppressed,
                "unavailable",
                "no connected session",
            );
        };
        if session
            .clock_sample()
            .is_some_and(|clock| clock.rate_ppm().get() == 0)
        {
            return failure(identity, Stage::Suppressed, "paused", "simulation paused");
        }
        if self.policy.pending.len() + self.policy.ready.len() >= 32 {
            return failure(
                identity,
                Stage::Rejected,
                "busy",
                "32 policy results are outstanding",
            );
        }
        if patch.apply(MovementPolicy::default()).is_none() {
            return failure(
                identity,
                Stage::Rejected,
                "invalid_value",
                "policy value outside the supported range",
            );
        }
        if let Err(error) = session.queue_world_input(
            minimium_protocol::ClientWorldInput::MovementPatch {
                patch,
                expected_revision,
            },
            now,
        ) {
            return failure(
                identity,
                Stage::Rejected,
                match error {
                    QueueError::BufferFull => "busy",
                    QueueError::StaleWorldContext => "stale_context",
                    _ => "unavailable",
                },
                &format!("{error:?}"),
            );
        }
        self.policy.pending.insert(id, (identity.clone(), patch));
        OperationResult::new(
            identity,
            OperationReceipt::at(Stage::Submitted),
            Ok(Payload::Value(V::Unit)),
        )
    }

    pub fn take_movement_results(&mut self) -> Vec<OperationResult> {
        std::mem::take(&mut self.policy.ready)
    }

    pub(super) fn observe_policy(&mut self, event: &SessionEvent, now: ClientTime) {
        match event {
            SessionEvent::Message(
                ServerMessage::Welcome { context, .. }
                | ServerMessage::TransitionCommitted { context, .. },
            ) => {
                self.policy.context = Some(*context);
                self.policy.sampled = None;
            }
            SessionEvent::Message(ServerMessage::MovementPolicy(policy)) => {
                self.install_policy(*policy, now)
            }
            SessionEvent::Message(ServerMessage::PolicyReply(ack)) => self.policy_reply(*ack, now),
            SessionEvent::Disconnected(_) | SessionEvent::Message(ServerMessage::Bye) => {
                self.policy.end_session()
            }
            _ => {}
        }
    }
    fn install_policy(&mut self, policy: MovementPolicy, now: ClientTime) {
        if self
            .policy
            .sampled
            .is_none_or(|(_, previous)| policy.revision >= previous.revision)
        {
            self.movement = policy;
            self.flight_enabled = policy.can_fly;
            self.policy.sampled = Some((now, policy));
        }
    }
    fn policy_reply(&mut self, ack: PolicyReply, now: ClientTime) {
        if let Ok(policy) = ack.result {
            if self.policy.context == Some(ack.context) {
                self.install_policy(policy, now);
            }
        }
        let Some((identity, _)) = self.policy.pending.get(&ack.request_id) else {
            return;
        };
        if identity.context != Some(binding(ack.context)) {
            return;
        }
        let (identity, _) = self
            .policy
            .pending
            .remove(&ack.request_id)
            .expect("pending request");
        let result = match ack.result {
            Ok(p) => {
                let mut receipt = OperationReceipt::at(Stage::OwnerCommitted);
                receipt.revision = Some(p.revision);
                OperationResult::new(
                    identity,
                    receipt,
                    Ok(Payload::Value(V::Record(
                        [
                            ("revision".into(), V::U64(p.revision)),
                            ("speed".into(), number(p.speed)),
                            ("accel".into(), number(p.accel)),
                            ("can_fly".into(), V::Bool(p.can_fly)),
                        ]
                        .into(),
                    ))),
                )
            }
            Err(reason) => {
                let code = match reason {
                    PolicyRejection::Forbidden => "forbidden",
                    PolicyRejection::StaleContext => "stale_context",
                    PolicyRejection::InvalidValue => "invalid_value",
                    PolicyRejection::RevisionConflict => "revision_conflict",
                    PolicyRejection::Busy => "busy",
                    PolicyRejection::Paused => "paused",
                };
                failure(
                    identity,
                    Stage::Rejected,
                    code,
                    "movement policy request rejected by its owner",
                )
            }
        };
        self.policy.ready.push(result);
    }
}
