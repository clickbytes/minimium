use crate::{
    environment_codec,
    framing::decode_frame,
    kinds::ServerKind,
    limits::*,
    models::{
        EntityReplicationOperation, ProtocolError, RemotePlayer, RemotePlayerIds, RemotePlayers,
        ServerMessage,
    },
    reading::Reader,
};
use minimium_core::ids::PlayerId;
use minimium_world::entity::{ComponentTypeId, EntityId, SchemaVersion};

pub fn decode_server(input: &[u8]) -> Result<Option<(ServerMessage, usize)>, ProtocolError> {
    let Some((kind, payload, frame_bytes)) = decode_frame(input)? else {
        return Ok(None);
    };
    let mut reader = Reader::new(payload);
    let message = match ServerKind::try_from(kind)? {
        ServerKind::Bolid => {
            let context = reader.world_context()?;
            let event = crate::BolidEvent {
                id: reader.u64()?,
                start_monotonic_ms: reader.u64()?,
                seed: reader.u64()?,
                start_world_m: reader.vector()?,
                end_world_m: reader.vector()?,
                duration_ms: reader.u16()?,
                kind: reader.byte()?,
            };
            if !event.valid() {
                return Err(ProtocolError::InvalidPayload);
            }
            ServerMessage::Bolid { context, event }
        }
        ServerKind::MovementPolicy => {
            let policy = minimium_simulation::MovementPolicy {
                can_fly: reader.boolean()?,
                speed: f32::from_le_bytes(reader.fixed()?),
                accel: f32::from_le_bytes(reader.fixed()?),
                revision: reader.u64()?,
            };
            if !policy.valid() {
                return Err(ProtocolError::InvalidPayload);
            }
            ServerMessage::MovementPolicy(policy)
        }
        ServerKind::EntitySyncBegin => {
            let context = reader.world_context()?;
            let sync_id = reader.u64()?;
            let base_revision = reader.u64()?;
            let target_revision = reader.u64()?;
            let full = reader.boolean()?;
            if sync_id == 0 || target_revision < base_revision || (full && base_revision != 0) {
                return Err(ProtocolError::InvalidPayload);
            }
            ServerMessage::EntitySyncBegin {
                context,
                sync_id,
                base_revision,
                target_revision,
                full,
            }
        }
        ServerKind::EntitySyncRecord => {
            let context = reader.world_context()?;
            let sync_id = reader.u64()?;
            let revision = reader.u64()?;
            if sync_id == 0 {
                return Err(ProtocolError::InvalidPayload);
            }
            let operation = match reader.byte()? {
                1 => EntityReplicationOperation::Spawn {
                    entity: read_entity(&mut reader)?,
                },
                2 => {
                    let entity = read_entity(&mut reader)?;
                    let kind =
                        ComponentTypeId::new(reader.u32()?).ok_or(ProtocolError::InvalidPayload)?;
                    let schema =
                        SchemaVersion::new(reader.u32()?).ok_or(ProtocolError::InvalidPayload)?;
                    let bytes = reader.bounded_bytes(MAX_COMPONENT_BYTES)?;
                    EntityReplicationOperation::ComponentUpsert {
                        entity,
                        kind,
                        schema,
                        bytes,
                    }
                }
                3 => EntityReplicationOperation::ComponentRemove {
                    entity: read_entity(&mut reader)?,
                    kind: ComponentTypeId::new(reader.u32()?)
                        .ok_or(ProtocolError::InvalidPayload)?,
                },
                4 => EntityReplicationOperation::Despawn {
                    entity: read_entity(&mut reader)?,
                },
                _ => return Err(ProtocolError::InvalidPayload),
            };
            ServerMessage::EntitySyncRecord {
                context,
                sync_id,
                revision,
                operation,
            }
        }
        ServerKind::EntitySyncComplete => {
            let context = reader.world_context()?;
            let sync_id = reader.u64()?;
            if sync_id == 0 {
                return Err(ProtocolError::InvalidPayload);
            }
            ServerMessage::EntitySyncComplete {
                context,
                sync_id,
                target_revision: reader.u64()?,
            }
        }
        ServerKind::ContentOffer => ServerMessage::ContentOffer {
            material_hash: reader.fixed()?,
            http_port: reader.u16()?,
            climate: crate::content_codec::read(&mut reader)?,
        },
        ServerKind::Welcome => {
            let id = PlayerId::new(reader.u64()?);
            let server_tick = reader.u64()?;
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
            let (clock_sample, environment) = environment_codec::read_pair(&mut reader)?;
            ServerMessage::Welcome {
                server_tick,
                id,
                map_fingerprint,
                health,
                position,
                position_mm,
                context,
                manifest,
                transition_id,
                clock_sample,
                environment,
            }
        }
        ServerKind::AltitudeResult => {
            let context = reader.world_context()?;
            let position_mm = if reader.boolean()? {
                Some(reader.position_mm()?)
            } else {
                None
            };
            ServerMessage::AltitudeResult {
                context,
                position_mm,
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
            let server_tick = reader.u64()?;
            let resolved_input_seq = reader.u64()?;
            let context = reader.world_context()?;
            let self_movement = crate::movement_codec::read(&mut reader)?;
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
                server_tick,
                resolved_input_seq,
                context,
                full_snapshot,
                self_movement,
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
            let manifest = reader.world_manifest()?;
            let (clock_sample, environment) = environment_codec::read_pair(&mut reader)?;
            ServerMessage::TransitionPrepare {
                transition_id,
                source_context,
                target_context,
                map_fingerprint,
                destination: destination_mm
                    .0
                    .map(|coordinate| coordinate as f32 / 1000.0),
                destination_mm,
                manifest,
                clock_sample,
                environment,
            }
        }
        ServerKind::TransitionCommitted => {
            let transition_id = reader.transition_id()?;
            let server_tick = reader.u64()?;
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
            let manifest = reader.world_manifest()?;
            let (clock_sample, environment) = environment_codec::read_pair(&mut reader)?;
            ServerMessage::TransitionCommitted {
                transition_id,
                server_tick,
                context,
                map_fingerprint,
                destination,
                destination_mm,
                health,
                manifest,
                clock_sample,
                environment,
            }
        }
        ServerKind::EnvironmentSync => {
            let context = reader.world_context()?;
            let (clock_sample, environment) = environment_codec::read_pair(&mut reader)?;
            ServerMessage::EnvironmentSync {
                context,
                clock_sample,
                environment,
            }
        }
        ServerKind::TimeProbeReply => {
            let seq = reader.u32()?;
            let echoed_client_send_monotonic_ms = reader.u64()?;
            let server_receive_monotonic_ms = reader.u64()?;
            let server_send_monotonic_ms = reader.u64()?;
            if server_send_monotonic_ms < server_receive_monotonic_ms {
                return Err(ProtocolError::InvalidPayload);
            }
            ServerMessage::TimeProbeReply {
                seq,
                echoed_client_send_monotonic_ms,
                server_receive_monotonic_ms,
                server_send_monotonic_ms,
                session_nonce: minimium_environment::time::SessionNonce::new(reader.fixed()?),
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

fn read_entity(reader: &mut Reader<'_>) -> Result<EntityId, ProtocolError> {
    EntityId::new(reader.u64()?).ok_or(ProtocolError::InvalidPayload)
}
