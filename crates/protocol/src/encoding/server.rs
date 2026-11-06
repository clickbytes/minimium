use super::common::{write_chat_text, write_name, write_vector, write_world_context};
use crate::{
    content_codec, environment_codec,
    kinds::ServerKind,
    limits::*,
    models::{ProtocolError, ServerMessage},
};

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
            ServerMessage::ContentOffer {
                material_hash,
                http_port,
                climate,
            } => {
                frame.extend_from_slice(material_hash);
                frame.extend_from_slice(&http_port.to_le_bytes());
                content_codec::write(climate, frame)?;
                ServerKind::ContentOffer
            }
            ServerMessage::Welcome {
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
            } => {
                if *health > 100 {
                    return Err(ProtocolError::InvalidPayload);
                }
                frame.extend_from_slice(&id.get().to_le_bytes());
                frame.extend_from_slice(&map_fingerprint.to_le_bytes());
                frame.extend_from_slice(&health.to_le_bytes());
                let _ = position;
                write_position_mm(*position_mm, frame);
                write_world_context(*context, frame);
                write_world_manifest(*manifest, frame);
                frame.push(u8::from(transition_id.is_some()));
                if let Some(transition_id) = transition_id {
                    frame.extend_from_slice(&transition_id.get().to_le_bytes());
                }
                environment_codec::write_pair(*clock_sample, *environment, frame)?;
                ServerKind::Welcome
            }
            ServerMessage::AltitudeResult {
                context,
                position_mm,
            } => {
                write_world_context(*context, frame);
                frame.push(u8::from(position_mm.is_some()));
                if let Some(position) = position_mm {
                    write_position_mm(*position, frame);
                }
                ServerKind::AltitudeResult
            }
            ServerMessage::Pong => ServerKind::Pong,
            ServerMessage::MapTeleportResult { position_mm } => {
                frame.push(u8::from(position_mm.is_some()));
                if let Some(position_mm) = position_mm {
                    write_position_mm(*position_mm, frame);
                }
                ServerKind::MapTeleportResult
            }
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
                context,
                full_snapshot,
                self_position,
                self_position_mm,
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
                write_world_context(*context, frame);
                let _ = self_position;
                write_position_mm(*self_position_mm, frame);
                write_vector(*self_velocity, frame)?;
                frame.push(u8::from(*self_grounded));
                frame.push(u8::from(*full_snapshot));
                frame.push(players.len() as u8);
                frame.push(removed_players.len() as u8);
                for player in players.iter() {
                    frame.extend_from_slice(&player.id.get().to_le_bytes());
                    write_name(&player.name, frame)?;
                    write_position_mm(player.position_mm, frame);
                }
                for id in removed_players.iter() {
                    frame.extend_from_slice(&id.get().to_le_bytes());
                }
                ServerKind::Snapshot
            }
            ServerMessage::TransitionPrepare {
                transition_id,
                source_context,
                target_context,
                map_fingerprint,
                destination,
                destination_mm,
                manifest,
                clock_sample,
                environment,
            } => {
                frame.extend_from_slice(&transition_id.get().to_le_bytes());
                write_world_context(*source_context, frame);
                write_world_context(*target_context, frame);
                frame.extend_from_slice(&map_fingerprint.to_le_bytes());
                let _ = destination;
                write_position_mm(*destination_mm, frame);
                write_world_manifest(*manifest, frame);
                environment_codec::write_pair(*clock_sample, *environment, frame)?;
                ServerKind::TransitionPrepare
            }
            ServerMessage::TransitionCommitted {
                transition_id,
                context,
                map_fingerprint,
                destination,
                destination_mm,
                health,
                manifest,
                clock_sample,
                environment,
            } => {
                if *health > 100 {
                    return Err(ProtocolError::InvalidPayload);
                }
                frame.extend_from_slice(&transition_id.get().to_le_bytes());
                write_world_context(*context, frame);
                frame.extend_from_slice(&map_fingerprint.to_le_bytes());
                let _ = destination;
                write_position_mm(*destination_mm, frame);
                frame.extend_from_slice(&health.to_le_bytes());
                write_world_manifest(*manifest, frame);
                environment_codec::write_pair(*clock_sample, *environment, frame)?;
                ServerKind::TransitionCommitted
            }
            ServerMessage::TransitionCancelled {
                transition_id,
                context,
            } => {
                frame.extend_from_slice(&transition_id.get().to_le_bytes());
                write_world_context(*context, frame);
                ServerKind::TransitionCancelled
            }
            ServerMessage::EnvironmentSync {
                context,
                clock_sample,
                environment,
            } => {
                write_world_context(*context, frame);
                environment_codec::write_pair(*clock_sample, *environment, frame)?;
                ServerKind::EnvironmentSync
            }
            ServerMessage::TimeProbeReply {
                seq,
                echoed_client_send_monotonic_ms,
                server_receive_monotonic_ms,
                server_send_monotonic_ms,
                session_nonce,
            } => {
                if server_send_monotonic_ms < server_receive_monotonic_ms {
                    return Err(ProtocolError::InvalidPayload);
                }
                frame.extend_from_slice(&seq.to_le_bytes());
                frame.extend_from_slice(&echoed_client_send_monotonic_ms.to_le_bytes());
                frame.extend_from_slice(&server_receive_monotonic_ms.to_le_bytes());
                frame.extend_from_slice(&server_send_monotonic_ms.to_le_bytes());
                frame.extend_from_slice(session_nonce.as_bytes());
                ServerKind::TimeProbeReply
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

pub(crate) fn write_position_mm(
    position: minimium_world::world::Position3Mm,
    output: &mut Vec<u8>,
) {
    for coordinate in position.0 {
        output.extend_from_slice(&coordinate.to_le_bytes());
    }
}

fn write_world_manifest(manifest: minimium_world::world::WorldManifest, output: &mut Vec<u8>) {
    output.extend_from_slice(&manifest.definition_id().get().to_le_bytes());
    output.push(match manifest.topology() {
        minimium_world::world::WorldTopology::Planet => 0,
        minimium_world::world::WorldTopology::Dungeon => 1,
    });
    let generation = manifest.generation();
    output.extend_from_slice(&generation.seed().get().to_le_bytes());
    output.extend_from_slice(&generation.revision().get().to_le_bytes());
    output.extend_from_slice(&generation.position_quantum_mm().to_le_bytes());
    output.extend_from_slice(&manifest.content_revision().get().to_le_bytes());
    output.extend_from_slice(manifest.generator_digest().as_bytes());
    if let Some(planet) = manifest.planet() {
        output.push(1);
        output.push(planet.mode);
        output.extend_from_slice(&planet.radius_mm.to_le_bytes());
        output.extend_from_slice(&planet.height_scale_permille.to_le_bytes());
        output.extend_from_slice(&planet.sea_level_mm.to_le_bytes());
        output.extend_from_slice(&planet.mountain_limit_mm.to_le_bytes());
        output.extend_from_slice(&planet.detail_strength_mm.to_le_bytes());
        output.extend_from_slice(&planet.detail_wavelength_mm.to_le_bytes());
        output.extend_from_slice(&planet.mountain_band_frequency.to_le_bytes());
        output.extend_from_slice(&planet.package_digest);
        output.extend_from_slice(&planet.peak_relief.start_m.to_le_bytes());
        output.extend_from_slice(&planet.peak_relief.full_m.to_le_bytes());
        output.extend_from_slice(&planet.peak_relief.multiplier_permille.to_le_bytes());
        output.extend_from_slice(&planet.peak_relief.exponent.to_le_bytes());
    } else {
        output.push(0);
    }
}
