use crate::{reading::Reader, ProtocolError, WorldContext};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlayerActionKind {
    Move,
    Chat,
    Name,
    Fire,
    GrantTools,
    Portal,
    Goto,
    Altitude,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlayerActionDecision {
    Accepted,
    Preparing(minimium_world::world::TransitionId),
    Committed,
    Rejected(u8),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlayerActionReceipt {
    pub sequence: u64,
    pub context: WorldContext,
    pub kind: PlayerActionKind,
    pub decision: PlayerActionDecision,
}

impl PlayerActionReceipt {
    pub(crate) fn write(self, out: &mut Vec<u8>) -> Result<(), ProtocolError> {
        if self.sequence == 0 {
            return Err(ProtocolError::InvalidPayload);
        }
        out.extend_from_slice(&self.sequence.to_le_bytes());
        super::encoding::common::write_world_context(self.context, out);
        out.push(match self.kind {
            PlayerActionKind::Move => 1,
            PlayerActionKind::Chat => 2,
            PlayerActionKind::Name => 3,
            PlayerActionKind::Fire => 4,
            PlayerActionKind::GrantTools => 5,
            PlayerActionKind::Portal => 6,
            PlayerActionKind::Goto => 7,
            PlayerActionKind::Altitude => 8,
        });
        let (tag, reason) = match self.decision {
            PlayerActionDecision::Accepted => (1, 0),
            PlayerActionDecision::Preparing(_) => (4, 0),
            PlayerActionDecision::Committed => (2, 0),
            PlayerActionDecision::Rejected(reason) if reason != 0 => (3, reason),
            _ => return Err(ProtocolError::InvalidPayload),
        };
        out.extend_from_slice(&[tag, reason]);
        if let PlayerActionDecision::Preparing(transition_id) = self.decision {
            if self.kind != PlayerActionKind::Portal {
                return Err(ProtocolError::InvalidPayload);
            }
            out.extend_from_slice(&transition_id.get().to_le_bytes());
        }
        Ok(())
    }

    pub(crate) fn read(reader: &mut Reader<'_>) -> Result<Self, ProtocolError> {
        let sequence = reader.u64()?;
        if sequence == 0 {
            return Err(ProtocolError::InvalidPayload);
        }
        let context = reader.world_context()?;
        let kind = match reader.byte()? {
            1 => PlayerActionKind::Move,
            2 => PlayerActionKind::Chat,
            3 => PlayerActionKind::Name,
            4 => PlayerActionKind::Fire,
            5 => PlayerActionKind::GrantTools,
            6 => PlayerActionKind::Portal,
            7 => PlayerActionKind::Goto,
            8 => PlayerActionKind::Altitude,
            _ => return Err(ProtocolError::InvalidPayload),
        };
        let tag = reader.byte()?;
        let reason = reader.byte()?;
        let decision = match (tag, reason) {
            (1, 0) => PlayerActionDecision::Accepted,
            (4, 0) if kind == PlayerActionKind::Portal => PlayerActionDecision::Preparing(
                minimium_world::world::TransitionId::new(reader.u128()?)
                    .ok_or(ProtocolError::InvalidPayload)?,
            ),
            (2, 0) => PlayerActionDecision::Committed,
            (3, reason) if reason != 0 => PlayerActionDecision::Rejected(reason),
            _ => return Err(ProtocolError::InvalidPayload),
        };
        Ok(Self {
            sequence,
            context,
            kind,
            decision,
        })
    }
}
