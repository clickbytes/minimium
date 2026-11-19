use super::policy::{binding, failure};
use super::*;
use minimium_commands::operation::CommandValue as V;
use minimium_commands::results::{
    OperationContext, OperationReceipt, OperationResult, Payload, Stage,
};
use minimium_protocol::{JumpOutcome, MotionRejection, MovementOutcome, MovementReceipt};
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct JumpRequests {
    pending: BTreeMap<u64, (OperationContext, u64, bool)>,
    pub(super) ready: Vec<OperationResult>,
}
impl JumpRequests {
    pub(super) fn end_session(&mut self) {
        for (_, (identity, _, _)) in std::mem::take(&mut self.pending) {
            self.ready.push(failure(
                identity,
                Stage::UnknownAfterDisconnect,
                "unknown_after_disconnect",
                "connection ended before the jump was resolved",
            ));
        }
    }
    pub(super) fn transfer(&mut self, context: minimium_protocol::WorldContext) {
        let current = Some(binding(context));
        self.pending.retain(|_, (identity, _, _)| {
            if identity.context == current {
                true
            } else {
                self.ready.push(failure(
                    identity.clone(),
                    Stage::Rejected,
                    "stale_context",
                    "the actor transferred before resolution",
                ));
                false
            }
        });
    }
    pub(super) fn receive(&mut self, ack: MovementReceipt) {
        let Some((identity, seq, accepted)) = self.pending.get_mut(&ack.request_id) else {
            return;
        };
        if *seq != ack.input_seq || identity.context != Some(binding(ack.context)) {
            return;
        }
        let identity = identity.clone();
        let MovementOutcome::Jump(outcome) = ack.outcome;
        let mut fields = BTreeMap::from([
            ("intent".into(), V::Enum("jump".into())),
            ("input_seq".into(), V::U64(ack.input_seq)),
            ("tick".into(), V::U64(ack.tick)),
        ]);
        let stage = match outcome {
            JumpOutcome::Accepted => {
                if *accepted {
                    return;
                }
                *accepted = true;
                Stage::OwnerAccepted
            }
            JumpOutcome::Resolved(decision)
            | JumpOutcome::Coalesced {
                resolution: decision,
                ..
            } => {
                self.pending.remove(&ack.request_id);
                fields.insert(
                    "resolution".into(),
                    V::Enum(
                        match decision {
                            minimium_simulation::JumpResolution::Applied => "applied",
                            minimium_simulation::JumpResolution::Swimming => "swimming",
                            minimium_simulation::JumpResolution::NotGrounded => "not_grounded",
                            minimium_simulation::JumpResolution::FlightMode => "flight_mode",
                        }
                        .into(),
                    ),
                );
                if let JumpOutcome::Coalesced {
                    applied_request_id, ..
                } = outcome
                {
                    fields.insert("outcome".into(), V::Enum("coalesced".into()));
                    fields.insert("applied_request_id".into(), V::U64(applied_request_id));
                } else {
                    fields.insert(
                        "outcome".into(),
                        V::Enum(
                            if decision == minimium_simulation::JumpResolution::Applied {
                                "applied"
                            } else {
                                "no_op"
                            }
                            .into(),
                        ),
                    );
                }
                Stage::Realized
            }
            JumpOutcome::Rejected(reason) => {
                self.pending.remove(&ack.request_id);
                let code = match reason {
                    MotionRejection::Forbidden => "forbidden",
                    MotionRejection::StaleContext => "stale_context",
                    MotionRejection::Busy => "busy",
                    MotionRejection::Paused => "paused",
                    MotionRejection::Superseded => "superseded",
                    MotionRejection::GeometryUnavailable => "geometry_unavailable",
                    MotionRejection::InvalidSequence => "invalid_sequence",
                };
                let mut result = failure(
                    identity,
                    Stage::Rejected,
                    code,
                    "jump rejected by the server",
                );
                result.receipt.effects.push(V::Record(fields));
                self.ready.push(result);
                return;
            }
        };
        self.ready.push(OperationResult::new(
            identity,
            OperationReceipt::at(stage),
            Ok(Payload::Value(V::Record(fields))),
        ));
    }
}
impl ConnectionController {
    pub fn request_jump(&mut self, now: ClientTime) -> OperationResult {
        let request_id = self
            .session()
            .map_or(1, ClientSession::next_world_request_id);
        let identity = self.policy_identity(request_id, "player.movement.jump");
        if let Err(error) = self.accept_time(now) {
            return failure(
                identity,
                Stage::Rejected,
                "invalid_time",
                &error.to_string(),
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
            .is_some_and(|sample| sample.rate_ppm().get() == 0)
        {
            return failure(identity, Stage::Suppressed, "paused", "simulation paused");
        }
        if self.jumps.pending.len() >= 32
            || self.jumps.ready.len() + self.jumps.pending.len() * 2 + 2 > 64
        {
            return failure(
                identity,
                Stage::Rejected,
                "busy",
                "jump receipt capacity exhausted",
            );
        }
        let input_seq = session.next_movement_input_seq();
        if let Err(error) =
            session.queue_world_input(minimium_protocol::ClientWorldInput::Jump { input_seq }, now)
        {
            return failure(
                identity,
                Stage::Rejected,
                if error == QueueError::BufferFull {
                    "busy"
                } else {
                    "unavailable"
                },
                &format!("{error:?}"),
            );
        }
        self.jumps
            .pending
            .insert(request_id, (identity.clone(), input_seq, false));
        OperationResult::new(
            identity,
            OperationReceipt::at(Stage::Submitted),
            Ok(Payload::Value(V::Record(
                [
                    ("intent".into(), V::Enum("jump".into())),
                    ("input_seq".into(), V::U64(input_seq)),
                ]
                .into(),
            ))),
        )
    }
}
