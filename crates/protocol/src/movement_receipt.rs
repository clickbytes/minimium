use crate::{reading::Reader, ProtocolError, WorldContext};
use minimium_simulation::JumpResolution;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MotionRejection {
    Forbidden,
    StaleContext,
    Busy,
    Paused,
    Superseded,
    GeometryUnavailable,
    InvalidSequence,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JumpOutcome {
    Accepted,
    Resolved(JumpResolution),
    Coalesced {
        applied_request_id: u64,
        resolution: JumpResolution,
    },
    Rejected(MotionRejection),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MovementOutcome {
    Jump(JumpOutcome),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MovementReceipt {
    pub request_id: u64,
    pub input_seq: u64,
    pub context: WorldContext,
    pub tick: u64,
    pub outcome: MovementOutcome,
}

fn tag(resolution: JumpResolution) -> u8 {
    match resolution {
        JumpResolution::Applied => 1,
        JumpResolution::NotGrounded => 2,
        JumpResolution::Swimming => 3,
        JumpResolution::FlightMode => 4,
    }
}
fn resolution(tag: u8) -> Result<JumpResolution, ProtocolError> {
    Ok(match tag {
        1 => JumpResolution::Applied,
        2 => JumpResolution::NotGrounded,
        3 => JumpResolution::Swimming,
        4 => JumpResolution::FlightMode,
        _ => return Err(ProtocolError::InvalidPayload),
    })
}
impl MovementReceipt {
    fn valid(self) -> bool {
        self.request_id != 0
            && self.input_seq != 0
            && !matches!(self.outcome,MovementOutcome::Jump(JumpOutcome::Coalesced { applied_request_id,.. }) if applied_request_id==0 || applied_request_id==self.request_id)
    }
    pub(crate) fn write(self, out: &mut Vec<u8>) -> Result<(), ProtocolError> {
        if !self.valid() {
            return Err(ProtocolError::InvalidPayload);
        }
        for value in [self.request_id, self.input_seq, self.tick] {
            out.extend_from_slice(&value.to_le_bytes());
        }
        match self.outcome {
            MovementOutcome::Jump(outcome) => {
                out.push(1);
                match outcome {
                    JumpOutcome::Accepted => out.push(1),
                    JumpOutcome::Resolved(value) => out.extend([2, tag(value)]),
                    JumpOutcome::Coalesced {
                        applied_request_id,
                        resolution,
                    } => {
                        out.extend([3, tag(resolution)]);
                        out.extend_from_slice(&applied_request_id.to_le_bytes());
                    }
                    JumpOutcome::Rejected(reason) => out.extend([
                        4,
                        match reason {
                            MotionRejection::Forbidden => 1,
                            MotionRejection::StaleContext => 2,
                            MotionRejection::Busy => 3,
                            MotionRejection::Paused => 4,
                            MotionRejection::Superseded => 5,
                            MotionRejection::GeometryUnavailable => 6,
                            MotionRejection::InvalidSequence => 7,
                        },
                    ]),
                }
            }
        }
        Ok(())
    }
    pub(crate) fn read(reader: &mut Reader<'_>) -> Result<Self, ProtocolError> {
        let context = reader.world_context()?;
        let request_id = reader.u64()?;
        let input_seq = reader.u64()?;
        let tick = reader.u64()?;
        let outcome = match reader.byte()? {
            1 => MovementOutcome::Jump(match reader.byte()? {
                1 => JumpOutcome::Accepted,
                2 => JumpOutcome::Resolved(resolution(reader.byte()?)?),
                3 => JumpOutcome::Coalesced {
                    resolution: resolution(reader.byte()?)?,
                    applied_request_id: reader.u64()?,
                },
                4 => JumpOutcome::Rejected(match reader.byte()? {
                    1 => MotionRejection::Forbidden,
                    2 => MotionRejection::StaleContext,
                    3 => MotionRejection::Busy,
                    4 => MotionRejection::Paused,
                    5 => MotionRejection::Superseded,
                    6 => MotionRejection::GeometryUnavailable,
                    7 => MotionRejection::InvalidSequence,
                    _ => return Err(ProtocolError::InvalidPayload),
                }),
                _ => return Err(ProtocolError::InvalidPayload),
            }),
            _ => return Err(ProtocolError::InvalidPayload),
        };
        let receipt = Self {
            context,
            request_id,
            input_seq,
            tick,
            outcome,
        };
        receipt
            .valid()
            .then_some(receipt)
            .ok_or(ProtocolError::InvalidPayload)
    }
}
