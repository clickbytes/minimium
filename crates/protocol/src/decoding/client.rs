use crate::{
    framing::decode_frame,
    kinds::ClientKind,
    models::{ClientMessage, ClientWorldInput, ProtocolError},
    reading::Reader,
    validation::is_unit_direction,
};

pub fn decode_client(input: &[u8]) -> Result<Option<(ClientMessage, usize)>, ProtocolError> {
    let Some((kind, payload, frame_bytes)) = decode_frame(input)? else {
        return Ok(None);
    };
    let mut reader = Reader::new(payload);
    let message = match ClientKind::try_from(kind)? {
        ClientKind::Presentation => ClientMessage::Presentation(match reader.byte()? {
            0 => None,
            1 => Some(true),
            2 => Some(false),
            _ => return Err(ProtocolError::InvalidPayload),
        }),
        ClientKind::ContentReady => ClientMessage::ContentReady(reader.fixed()?),
        ClientKind::Identify => ClientMessage::Identify(reader.profile_id()?),
        ClientKind::Ping => ClientMessage::Ping,
        ClientKind::SetName => ClientMessage::SetName(reader.name()?),
        ClientKind::Move => ClientMessage::Move {
            direction: reader.direction()?,
            jump: reader.boolean()?,
        },
        ClientKind::Fire => {
            let direction = reader.vector()?;
            if !is_unit_direction(direction) {
                return Err(ProtocolError::InvalidPayload);
            }
            ClientMessage::Fire { direction }
        }
        ClientKind::Chat => ClientMessage::Chat(reader.chat_text()?.to_owned()),
        ClientKind::WorldInput => {
            let context = reader.world_context()?;
            let input = match reader.byte()? {
                1 => ClientWorldInput::SetName(reader.name()?),
                2 => ClientWorldInput::Move {
                    direction: reader.direction()?,
                    jump: reader.boolean()?,
                },
                3 => {
                    let direction = reader.vector()?;
                    if !is_unit_direction(direction) {
                        return Err(ProtocolError::InvalidPayload);
                    }
                    ClientWorldInput::Fire { direction }
                }
                4 => ClientWorldInput::Chat(reader.chat_text()?.to_owned()),
                5 => ClientWorldInput::UsePortal,
                6 => {
                    let direction = reader.vector()?;
                    if !is_unit_direction(direction) {
                        return Err(ProtocolError::InvalidPayload);
                    }
                    ClientWorldInput::MapTeleport { direction }
                }
                8 => {
                    let km = f32::from_le_bytes(reader.fixed()?);
                    if !km.is_finite() || !(0.0..=10000.0).contains(&km) {
                        return Err(ProtocolError::InvalidPayload);
                    }
                    ClientWorldInput::SetAltitude { km }
                }
                7 => ClientWorldInput::Motion(crate::movement_codec::read(&mut reader)?),
                _ => return Err(ProtocolError::InvalidPayload),
            };
            ClientMessage::WorldInput { context, input }
        }
        ClientKind::TransitionReady => ClientMessage::TransitionReady {
            transition_id: reader.transition_id()?,
            target_context: reader.world_context()?,
        },
        ClientKind::UsePortal => ClientMessage::UsePortal,
        ClientKind::CancelTransition => ClientMessage::CancelTransition(reader.transition_id()?),
        ClientKind::TimeProbe => ClientMessage::TimeProbe {
            seq: reader.u32()?,
            client_send_monotonic_ms: reader.u64()?,
        },
        ClientKind::Quit => ClientMessage::Quit,
    };
    reader.finish()?;
    Ok(Some((message, frame_bytes)))
}
