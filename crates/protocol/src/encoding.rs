use super::{
    framing::encode_frame,
    kinds::{ClientKind, ServerKind},
    limits::*,
    models::{ClientMessage, ProtocolError, ServerMessage},
    validation::{valid_chat, valid_direction_component, valid_name},
};

pub fn encode_client(message: &ClientMessage) -> Result<Vec<u8>, ProtocolError> {
    let mut payload = Vec::with_capacity(16);
    let kind = match message {
        ClientMessage::Identify(profile_id) => {
            payload.extend_from_slice(profile_id.as_bytes());
            ClientKind::Identify
        }
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
        ClientMessage::Chat(text) => {
            write_chat_text(text, &mut payload)?;
            ClientKind::Chat
        }
        ClientMessage::Quit => ClientKind::Quit,
    };
    encode_frame(kind.into(), &payload)
}

pub fn encode_server(message: &ServerMessage) -> Result<Vec<u8>, ProtocolError> {
    let mut frame = Vec::with_capacity(128);
    encode_server_into(&mut frame, message)?;
    Ok(frame)
}

pub fn encode_server_into(
    frame: &mut Vec<u8>,
    message: &ServerMessage,
) -> Result<(), ProtocolError> {
    let frame_start = frame.len();
    frame.extend_from_slice(&[0; HEADER_BYTES]);
    let payload_start = frame.len();
    let encoded = (|| {
        let kind = match message {
            ServerMessage::Welcome {
                id,
                map_fingerprint,
                health,
            } => {
                if *health > 100 {
                    return Err(ProtocolError::InvalidPayload);
                }
                frame.extend_from_slice(&id.get().to_le_bytes());
                frame.extend_from_slice(&map_fingerprint.to_le_bytes());
                frame.extend_from_slice(&health.to_le_bytes());
                ServerKind::Welcome
            }
            ServerMessage::Pong => ServerKind::Pong,
            ServerMessage::Accepted => ServerKind::Accepted,
            ServerMessage::Bye => ServerKind::Bye,
            ServerMessage::Error(code) => {
                frame.push((*code).into());
                ServerKind::Error
            }
            ServerMessage::ShotResult {
                target_id,
                remaining_health,
                killed,
            } => {
                if target_id.is_some() != remaining_health.is_some()
                    || (*killed && target_id.is_none())
                {
                    return Err(ProtocolError::InvalidPayload);
                }
                frame.push(u8::from(target_id.is_some()));
                if let (Some(id), Some(health)) = (target_id, remaining_health) {
                    frame.extend_from_slice(&id.get().to_le_bytes());
                    frame.extend_from_slice(&health.to_le_bytes());
                }
                frame.push(u8::from(*killed));
                ServerKind::ShotResult
            }
            ServerMessage::HealthUpdate(health) => {
                if *health > 100 {
                    return Err(ProtocolError::InvalidPayload);
                }
                frame.extend_from_slice(&health.to_le_bytes());
                ServerKind::HealthUpdate
            }
            ServerMessage::Chat {
                sender_id,
                sender_name,
                text,
            } => {
                frame.extend_from_slice(&sender_id.get().to_le_bytes());
                write_name(sender_name, frame)?;
                write_chat_text(text, frame)?;
                ServerKind::Chat
            }
            ServerMessage::Snapshot {
                server_time_ms,
                full_snapshot,
                self_position,
                self_velocity,
                self_grounded,
                players,
                removed_players,
            } => {
                if (*full_snapshot && !removed_players.is_empty())
                    || players
                        .iter()
                        .any(|player| removed_players.contains(player.id))
                {
                    return Err(ProtocolError::InvalidPayload);
                }
                frame.extend_from_slice(&server_time_ms.to_le_bytes());
                write_vector(*self_position, frame)?;
                write_vector(*self_velocity, frame)?;
                frame.push(u8::from(*self_grounded));
                frame.push(u8::from(*full_snapshot));
                frame.push(players.len() as u8);
                frame.push(removed_players.len() as u8);
                for player in players.iter() {
                    frame.extend_from_slice(&player.id.get().to_le_bytes());
                    write_name(&player.name, frame)?;
                    write_vector(player.position, frame)?;
                }
                for id in removed_players.iter() {
                    frame.extend_from_slice(&id.get().to_le_bytes());
                }
                ServerKind::Snapshot
            }
        };
        Ok(kind)
    })();

    let kind = match encoded {
        Ok(kind) => kind,
        Err(error) => {
            frame.truncate(frame_start);
            return Err(error);
        }
    };
    let payload_bytes = frame.len() - payload_start;
    if payload_bytes > MAX_PAYLOAD_BYTES || payload_bytes > u16::MAX as usize {
        frame.truncate(frame_start);
        return Err(ProtocolError::PayloadTooLarge);
    }
    frame[frame_start] = PROTOCOL_VERSION;
    frame[frame_start + 1] = kind.into();
    frame[frame_start + 2..frame_start + HEADER_BYTES]
        .copy_from_slice(&(payload_bytes as u16).to_le_bytes());
    Ok(())
}

pub(crate) fn write_name(name: &str, output: &mut Vec<u8>) -> Result<(), ProtocolError> {
    if !valid_name(name) {
        return Err(ProtocolError::InvalidName);
    }
    output.push(name.len() as u8);
    output.extend_from_slice(name.as_bytes());
    Ok(())
}

pub(crate) fn write_chat_text(text: &str, output: &mut Vec<u8>) -> Result<(), ProtocolError> {
    if !valid_chat(text) {
        return Err(ProtocolError::InvalidPayload);
    }
    output.push(text.len() as u8);
    output.extend_from_slice(text.as_bytes());
    Ok(())
}

pub(crate) fn write_vector(vector: [f32; 3], output: &mut Vec<u8>) -> Result<(), ProtocolError> {
    if !vector.iter().all(|value| value.is_finite()) {
        return Err(ProtocolError::InvalidPayload);
    }
    for value in vector {
        output.extend_from_slice(&value.to_le_bytes());
    }
    Ok(())
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

pub(crate) fn write_unit_direction(
    direction: [f32; 3],
    output: &mut Vec<u8>,
) -> Result<(), ProtocolError> {
    if !is_unit_direction(direction) {
        return Err(ProtocolError::InvalidPayload);
    }
    write_vector(direction, output)
}

pub(crate) fn is_unit_direction(direction: [f32; 3]) -> bool {
    if direction.iter().any(|value| !value.is_finite()) {
        return false;
    }
    let length_squared = direction.iter().map(|value| value * value).sum::<f32>();
    (0.99..=1.01).contains(&length_squared)
}
