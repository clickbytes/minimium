//! Movement policy changes use the WorldInput sequence as the request ID.
use crate::{reading::Reader, ProtocolError, WorldContext};
use minimium_simulation::MovementPolicy;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MovementPatch {
    Speed(f32),
    Acceleration(f32),
    FlightEnabled(bool),
    SetBoth { speed: f32, accel: f32 },
}

impl MovementPatch {
    pub fn apply(self, previous: MovementPolicy) -> Option<MovementPolicy> {
        let mut policy = previous;
        match self {
            Self::Speed(speed) => policy.speed = speed,
            Self::Acceleration(accel) => policy.accel = accel,
            Self::FlightEnabled(enabled) => policy.can_fly = enabled,
            Self::SetBoth { speed, accel } => {
                policy.speed = speed;
                policy.accel = accel;
            }
        }
        policy.valid().then_some(policy)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolicyRejection {
    Forbidden,
    StaleContext,
    InvalidValue,
    RevisionConflict,
    Busy,
    Paused,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PolicyReply {
    pub context: WorldContext,
    pub request_id: u64,
    pub result: Result<MovementPolicy, PolicyRejection>,
}

pub(crate) fn write_patch(
    out: &mut Vec<u8>,
    patch: MovementPatch,
    expected: Option<u64>,
) -> Result<(), ProtocolError> {
    if patch.apply(MovementPolicy::default()).is_none() {
        return Err(ProtocolError::InvalidPayload);
    }
    out.push(u8::from(expected.is_some()));
    if let Some(revision) = expected {
        out.extend_from_slice(&revision.to_le_bytes());
    }
    match patch {
        MovementPatch::Speed(speed) => {
            out.push(1);
            out.extend_from_slice(&speed.to_le_bytes());
        }
        MovementPatch::Acceleration(accel) => {
            out.push(2);
            out.extend_from_slice(&accel.to_le_bytes());
        }
        MovementPatch::FlightEnabled(enabled) => {
            out.extend([3, u8::from(enabled)]);
        }
        MovementPatch::SetBoth { speed, accel } => {
            out.push(4);
            out.extend_from_slice(&speed.to_le_bytes());
            out.extend_from_slice(&accel.to_le_bytes());
        }
    }
    Ok(())
}
pub(crate) fn read_patch(
    reader: &mut Reader<'_>,
) -> Result<crate::ClientWorldInput, ProtocolError> {
    let expected_revision = reader.boolean()?.then(|| reader.u64()).transpose()?;
    let patch = match reader.byte()? {
        1 => MovementPatch::Speed(f32::from_le_bytes(reader.fixed()?)),
        2 => MovementPatch::Acceleration(f32::from_le_bytes(reader.fixed()?)),
        3 => MovementPatch::FlightEnabled(reader.boolean()?),
        4 => MovementPatch::SetBoth {
            speed: f32::from_le_bytes(reader.fixed()?),
            accel: f32::from_le_bytes(reader.fixed()?),
        },
        _ => return Err(ProtocolError::InvalidPayload),
    };
    // Shape is valid. The authority returns a correlated InvalidValue for invalid numbers.
    Ok(crate::ClientWorldInput::MovementPatch {
        patch,
        expected_revision,
    })
}
pub(crate) fn write_reply(out: &mut Vec<u8>, ack: &PolicyReply) -> Result<(), ProtocolError> {
    if ack.request_id == 0 {
        return Err(ProtocolError::InvalidPayload);
    }
    out.extend_from_slice(&ack.request_id.to_le_bytes());
    match ack.result {
        Ok(policy) => {
            if !policy.valid() {
                return Err(ProtocolError::InvalidPayload);
            }
            out.extend([0, u8::from(policy.can_fly)]);
            out.extend_from_slice(&policy.speed.to_le_bytes());
            out.extend_from_slice(&policy.accel.to_le_bytes());
            out.extend_from_slice(&policy.revision.to_le_bytes());
        }
        Err(reason) => out.push(match reason {
            PolicyRejection::Forbidden => 1,
            PolicyRejection::StaleContext => 2,
            PolicyRejection::InvalidValue => 3,
            PolicyRejection::RevisionConflict => 4,
            PolicyRejection::Busy => 5,
            PolicyRejection::Paused => 6,
        }),
    }
    Ok(())
}
pub(crate) fn read_reply(reader: &mut Reader<'_>) -> Result<PolicyReply, ProtocolError> {
    let context = reader.world_context()?;
    let request_id = reader.u64()?;
    if request_id == 0 {
        return Err(ProtocolError::InvalidPayload);
    }
    let result = match reader.byte()? {
        0 => {
            let policy = MovementPolicy {
                can_fly: reader.boolean()?,
                speed: f32::from_le_bytes(reader.fixed()?),
                accel: f32::from_le_bytes(reader.fixed()?),
                revision: reader.u64()?,
            };
            if !policy.valid() {
                return Err(ProtocolError::InvalidPayload);
            }
            Ok(policy)
        }
        1 => Err(PolicyRejection::Forbidden),
        2 => Err(PolicyRejection::StaleContext),
        3 => Err(PolicyRejection::InvalidValue),
        4 => Err(PolicyRejection::RevisionConflict),
        5 => Err(PolicyRejection::Busy),
        6 => Err(PolicyRejection::Paused),
        _ => return Err(ProtocolError::InvalidPayload),
    };
    Ok(PolicyReply {
        request_id,
        context,
        result,
    })
}

impl crate::ClientWorldInput {
    pub fn policy_change(&self) -> Option<(MovementPatch, Option<u64>)> {
        Some(match *self {
            Self::MovementPatch {
                patch,
                expected_revision,
            } => (patch, expected_revision),
            Self::SetMovementPolicy { speed, accel } => {
                (MovementPatch::SetBoth { speed, accel }, None)
            }
            Self::SetFlightEnabled(enabled) => (MovementPatch::FlightEnabled(enabled), None),
            _ => return None,
        })
    }
}
