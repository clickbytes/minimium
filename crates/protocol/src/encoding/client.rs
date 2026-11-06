use super::common::{write_chat_text, write_name, write_vector, write_world_context};
use crate::{
    framing::encode_frame,
    kinds::ClientKind,
    models::{ClientMessage, ClientWorldInput, ProtocolError},
    validation::{is_unit_direction, valid_direction_component},
};

pub fn encode_client(message: &ClientMessage) -> Result<Vec<u8>, ProtocolError> {
    let mut payload = Vec::with_capacity(16);
    let kind = match message {
        ClientMessage::Presentation(presentation) => {
            payload.push(match presentation {
                None => 0,
                Some(true) => 1,
                Some(false) => 2,
            });
            ClientKind::Presentation
        }
        ClientMessage::ContentReady(hash) => {
            payload.extend_from_slice(hash);
            ClientKind::ContentReady
        }
        ClientMessage::Identify(profile_id) => {
            payload.extend_from_slice(profile_id.as_bytes());
            ClientKind::Identify
        }
        ClientMessage::Motion(_) => return Err(ProtocolError::InvalidPayload),
        ClientMessage::Ping => ClientKind::Ping,
        ClientMessage::SetName(name) => {
            write_name(name, &mut payload)?;
            ClientKind::SetName
        }
        ClientMessage::Move { direction, jump } => {
            write_direction(*direction, &mut payload)?;
            payload.push(u8::from(*jump));
            ClientKind::Move
        }
        ClientMessage::Fire { direction } => {
            write_unit_direction(*direction, &mut payload)?;
            ClientKind::Fire
        }
        ClientMessage::SetAltitude { .. } | ClientMessage::MapTeleport { .. } => {
            return Err(ProtocolError::InvalidPayload)
        }
        ClientMessage::Chat(text) => {
            write_chat_text(text, &mut payload)?;
            ClientKind::Chat
        }
        ClientMessage::WorldInput { context, input } => {
            write_world_context(*context, &mut payload);
            match input {
                ClientWorldInput::Motion(input) => {
                    payload.push(7);
                    crate::movement_codec::write(*input, &mut payload)?;
                }
                ClientWorldInput::SetName(name) => {
                    payload.push(1);
                    write_name(name, &mut payload)?;
                }
                ClientWorldInput::Move { direction, jump } => {
                    payload.push(2);
                    write_direction(*direction, &mut payload)?;
                    payload.push(u8::from(*jump));
                }
                ClientWorldInput::Fire { direction } => {
                    payload.push(3);
                    write_unit_direction(*direction, &mut payload)?;
                }
                ClientWorldInput::Chat(text) => {
                    payload.push(4);
                    write_chat_text(text, &mut payload)?;
                }
                ClientWorldInput::SetAltitude { km } => {
                    if !km.is_finite() || !(0.0..=10000.0).contains(km) {
                        return Err(ProtocolError::InvalidPayload);
                    }
                    payload.push(8);
                    payload.extend_from_slice(&km.to_le_bytes());
                }
                ClientWorldInput::UsePortal => payload.push(5),
                ClientWorldInput::MapTeleport { direction } => {
                    payload.push(6);
                    write_unit_direction(*direction, &mut payload)?;
                }
            }
            ClientKind::WorldInput
        }
        ClientMessage::TransitionReady {
            transition_id,
            target_context,
        } => {
            payload.extend_from_slice(&transition_id.get().to_le_bytes());
            write_world_context(*target_context, &mut payload);
            ClientKind::TransitionReady
        }
        ClientMessage::UsePortal => ClientKind::UsePortal,
        ClientMessage::CancelTransition(transition_id) => {
            payload.extend_from_slice(&transition_id.get().to_le_bytes());
            ClientKind::CancelTransition
        }
        ClientMessage::TimeProbe {
            seq,
            client_send_monotonic_ms,
        } => {
            payload.extend_from_slice(&seq.to_le_bytes());
            payload.extend_from_slice(&client_send_monotonic_ms.to_le_bytes());
            ClientKind::TimeProbe
        }
        ClientMessage::Quit => ClientKind::Quit,
    };
    encode_frame(kind.into(), &payload)
}

pub(crate) fn write_direction(
    direction: [f32; 2],
    output: &mut Vec<u8>,
) -> Result<(), ProtocolError> {
    if !direction
        .iter()
        .all(|value| valid_direction_component(*value))
    {
        return Err(ProtocolError::InvalidPayload);
    }
    for value in direction {
        output.extend_from_slice(&value.to_le_bytes());
    }
    Ok(())
}

fn write_unit_direction(direction: [f32; 3], output: &mut Vec<u8>) -> Result<(), ProtocolError> {
    if !is_unit_direction(direction) {
        return Err(ProtocolError::InvalidPayload);
    }
    write_vector(direction, output)
}
