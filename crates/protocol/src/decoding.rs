use super::{
    encoding::is_unit_direction,
    framing::decode_frame,
    kinds::{ClientKind, ServerKind},
    limits::*,
    models::{
        ClientMessage, ProtocolError, RemotePlayer, RemotePlayerIds, RemotePlayers, ServerMessage,
    },
    reading::Reader,
};
use minimium_core::ids::PlayerId;

pub fn decode_client(input: &[u8]) -> Result<Option<(ClientMessage, usize)>, ProtocolError> {
    let Some((kind, payload, frame_bytes)) = decode_frame(input)? else {
        return Ok(None);
    };
    let mut reader = Reader::new(payload);
    let message = match ClientKind::try_from(kind)? {
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
                1 => super::models::ClientWorldInput::SetName(reader.name()?),
                2 => super::models::ClientWorldInput::Move {
                    direction: reader.direction()?,
                    jump: reader.boolean()?,
                },
                3 => {
                    let direction = reader.vector()?;
                    if !is_unit_direction(direction) {
                        return Err(ProtocolError::InvalidPayload);
                    }
                    super::models::ClientWorldInput::Fire { direction }
                }
                4 => super::models::ClientWorldInput::Chat(reader.chat_text()?.to_owned()),
                5 => super::models::ClientWorldInput::UsePortal,
                6 => {
                    let direction = reader.vector()?;
                    if !is_unit_direction(direction) {
                        return Err(ProtocolError::InvalidPayload);
                    }
                    super::models::ClientWorldInput::MapTeleport { direction }
                }
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
        ClientKind::Quit => ClientMessage::Quit,
    };
    reader.finish()?;
    Ok(Some((message, frame_bytes)))
}

pub fn decode_server(input: &[u8]) -> Result<Option<(ServerMessage, usize)>, ProtocolError> {
    let Some((kind, payload, frame_bytes)) = decode_frame(input)? else {
        return Ok(None);
    };
    let mut reader = Reader::new(payload);
    let message = match ServerKind::try_from(kind)? {
        ServerKind::Welcome => {
            let id = PlayerId::new(reader.u64()?);
            let map_fingerprint = reader.u64()?;
            let health = reader.u16()?;
            let position_mm = reader.position_mm()?;
            let position = position_mm.0.map(|coordinate| coordinate as f32 / 1000.0);
            if health > 100 {
                return Err(ProtocolError::InvalidPayload);
            }
            let context = reader.world_context()?;
            let manifest = reader.world_manifest()?;
            let transition_id = if reader.boolean()? {
                Some(reader.transition_id()?)
            } else {
                None
            };
            ServerMessage::Welcome {
                id,
                map_fingerprint,
                health,
                position,
                position_mm,
                context,
                manifest,
                transition_id,
            }
        }
        ServerKind::Pong => ServerMessage::Pong,
        ServerKind::MapTeleportResult => {
            let position_mm = if reader.boolean()? {
                Some(reader.position_mm()?)
            } else {
                None
            };
            ServerMessage::MapTeleportResult { position_mm }
        }
        ServerKind::Accepted => ServerMessage::Accepted,
        ServerKind::Bye => ServerMessage::Bye,
        ServerKind::Error => ServerMessage::Error(reader.byte()?.into()),
        ServerKind::ShotResult => {
            let hit = reader.boolean()?;
            let (target_id, remaining_health) = if hit {
                let id = PlayerId::new(reader.u64()?);
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
        ServerKind::HealthUpdate => {
            let health = reader.u16()?;
            if health > 100 {
                return Err(ProtocolError::InvalidPayload);
            }
            ServerMessage::HealthUpdate(health)
        }
        ServerKind::Chat => ServerMessage::Chat {
            sender_id: PlayerId::new(reader.u64()?),
            sender_name: reader.name()?,
            text: reader.chat_text()?.to_owned(),
        },
        ServerKind::Snapshot => {
            let server_time_ms = reader.u64()?;
            let context = reader.world_context()?;
            let self_position_mm = reader.position_mm()?;
            let self_position = self_position_mm
                .0
                .map(|coordinate| coordinate as f32 / 1000.0);
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
                let id = PlayerId::new(reader.u64()?);
                let name = reader.shared_name()?;
                let position_mm = reader.position_mm()?;
                if !players.push(RemotePlayer {
                    id,
                    name,
                    position: position_mm.0.map(|coordinate| coordinate as f32 / 1000.0),
                    position_mm,
                }) {
                    return Err(ProtocolError::InvalidPayload);
                }
            }
            let mut removed_players = RemotePlayerIds::default();
            for _ in 0..removed_count {
                if !removed_players.push(PlayerId::new(reader.u64()?)) {
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
                context,
                full_snapshot,
                self_position,
                self_position_mm,
                self_velocity,
                self_grounded,
                players: Box::new(players),
                removed_players,
            }
        }
        ServerKind::TransitionPrepare => {
            let transition_id = reader.transition_id()?;
            let source_context = reader.world_context()?;
            let target_context = reader.world_context()?;
            let map_fingerprint = reader.u64()?;
            let destination_mm = reader.position_mm()?;
            ServerMessage::TransitionPrepare {
                transition_id,
                source_context,
                target_context,
                map_fingerprint,
                destination: destination_mm
                    .0
                    .map(|coordinate| coordinate as f32 / 1000.0),
                destination_mm,
                manifest: reader.world_manifest()?,
            }
        }
        ServerKind::TransitionCommitted => {
            let transition_id = reader.transition_id()?;
            let context = reader.world_context()?;
            let map_fingerprint = reader.u64()?;
            let destination_mm = reader.position_mm()?;
            let destination = destination_mm
                .0
                .map(|coordinate| coordinate as f32 / 1000.0);
            let health = reader.u16()?;
            if health > 100 {
                return Err(ProtocolError::InvalidPayload);
            }
            ServerMessage::TransitionCommitted {
                transition_id,
                context,
                map_fingerprint,
                destination,
                destination_mm,
                health,
                manifest: reader.world_manifest()?,
            }
        }
        ServerKind::TransitionCancelled => ServerMessage::TransitionCancelled {
            transition_id: reader.transition_id()?,
            context: reader.world_context()?,
        },
    };
    reader.finish()?;
    Ok(Some((message, frame_bytes)))
}
