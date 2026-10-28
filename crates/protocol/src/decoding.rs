use super::{
    encoding::is_unit_direction,
    framing::decode_frame,
    limits::*,
    models::{
        ClientMessage, ProtocolError, RemotePlayer, RemotePlayerIds, RemotePlayers, ServerMessage,
    },
    reading::Reader,
};

pub fn decode_client(input: &[u8]) -> Result<Option<(ClientMessage, usize)>, ProtocolError> {
    let Some((kind, payload, frame_bytes)) = decode_frame(input)? else {
        return Ok(None);
    };
    let mut reader = Reader::new(payload);
    let message = match kind {
        CLIENT_IDENTIFY => ClientMessage::Identify(reader.profile_id()?),
        CLIENT_PING => ClientMessage::Ping,
        CLIENT_SET_NAME => ClientMessage::SetName(reader.name()?),
        CLIENT_MOVE => ClientMessage::Move {
            direction: reader.direction()?,
            jump: reader.boolean()?,
        },
        CLIENT_FIRE => {
            let direction = reader.vector()?;
            if !is_unit_direction(direction) {
                return Err(ProtocolError::InvalidPayload);
            }
            ClientMessage::Fire { direction }
        }
        CLIENT_CHAT => ClientMessage::Chat(reader.chat_text()?.to_owned()),
        CLIENT_QUIT => ClientMessage::Quit,
        other => return Err(ProtocolError::UnknownMessage(other)),
    };
    reader.finish()?;
    Ok(Some((message, frame_bytes)))
}

pub fn decode_server(input: &[u8]) -> Result<Option<(ServerMessage, usize)>, ProtocolError> {
    let Some((kind, payload, frame_bytes)) = decode_frame(input)? else {
        return Ok(None);
    };
    let mut reader = Reader::new(payload);
    let message = match kind {
        SERVER_WELCOME => {
            let id = reader.u64()?;
            let map_fingerprint = reader.u64()?;
            let health = reader.u16()?;
            if health > 100 {
                return Err(ProtocolError::InvalidPayload);
            }
            ServerMessage::Welcome {
                id,
                map_fingerprint,
                health,
            }
        }
        SERVER_PONG => ServerMessage::Pong,
        SERVER_ACCEPTED => ServerMessage::Accepted,
        SERVER_BYE => ServerMessage::Bye,
        SERVER_ERROR => ServerMessage::Error(reader.byte()?),
        SERVER_SHOT_RESULT => {
            let hit = reader.boolean()?;
            let (target_id, remaining_health) = if hit {
                let id = reader.u64()?;
                let health = reader.u16()?;
                if health > 100 {
                    return Err(ProtocolError::InvalidPayload);
                }
                (Some(id), Some(health))
            } else {
                (None, None)
            };
            let killed = reader.boolean()?;
            if killed && !hit {
                return Err(ProtocolError::InvalidPayload);
            }
            ServerMessage::ShotResult {
                target_id,
                remaining_health,
                killed,
            }
        }
        SERVER_HEALTH_UPDATE => {
            let health = reader.u16()?;
            if health > 100 {
                return Err(ProtocolError::InvalidPayload);
            }
            ServerMessage::HealthUpdate(health)
        }
        SERVER_CHAT => ServerMessage::Chat {
            sender_id: reader.u64()?,
            sender_name: reader.name()?,
            text: reader.chat_text()?.to_owned(),
        },
        SERVER_SNAPSHOT => {
            let server_time_ms = reader.u64()?;
            let self_position = reader.vector()?;
            let self_velocity = reader.vector()?;
            let self_grounded = reader.boolean()?;
            let full_snapshot = reader.boolean()?;
            let count = reader.byte()? as usize;
            let removed_count = reader.byte()? as usize;
            if count > MAX_VISIBLE_PLAYERS
                || removed_count > MAX_VISIBLE_PLAYERS
                || (full_snapshot && removed_count > 0)
            {
                return Err(ProtocolError::InvalidPayload);
            }
            let mut players = RemotePlayers::default();
            for _ in 0..count {
                if !players.push(RemotePlayer {
                    id: reader.u64()?,
                    name: reader.shared_name()?,
                    position: reader.vector()?,
                }) {
                    return Err(ProtocolError::InvalidPayload);
                }
            }
            let mut removed_players = RemotePlayerIds::default();
            for _ in 0..removed_count {
                if !removed_players.push(reader.u64()?) {
                    return Err(ProtocolError::InvalidPayload);
                }
            }
            if players
                .iter()
                .any(|player| removed_players.contains(player.id))
            {
                return Err(ProtocolError::InvalidPayload);
            }
            ServerMessage::Snapshot {
                server_time_ms,
                full_snapshot,
                self_position,
                self_velocity,
                self_grounded,
                players,
                removed_players,
            }
        }
        other => return Err(ProtocolError::UnknownMessage(other)),
    };
    reader.finish()?;
    Ok(Some((message, frame_bytes)))
}
