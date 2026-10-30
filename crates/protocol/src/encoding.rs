use super::{
    environment_codec,
    framing::encode_frame,
    kinds::{ClientKind, ServerKind},
    limits::*,
    models::{ClientMessage, ClientWorldInput, ProtocolError, ServerMessage},
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
        ClientMessage::MapTeleport { .. } => return Err(ProtocolError::InvalidPayload),
        ClientMessage::Chat(text) => {
            write_chat_text(text, &mut payload)?;
            ClientKind::Chat
        }
        ClientMessage::WorldInput { context, input } => {
            write_world_context(*context, &mut payload);
            match input {
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

pub(crate) fn write_name(name: &str, output: &mut Vec<u8>) -> Result<(), ProtocolError> {
    if !valid_name(name) {
        return Err(ProtocolError::InvalidName);
    }
    output.push(name.len() as u8);
    output.extend_from_slice(name.as_bytes());
    Ok(())
}

fn write_world_context(context: super::world_context::WorldContext, output: &mut Vec<u8>) {
    output.extend_from_slice(&context.instance().get().to_le_bytes());
    output.extend_from_slice(&context.content_revision().get().to_le_bytes());
    output.extend_from_slice(&context.membership_epoch().get().to_le_bytes());
}

fn write_world_manifest(manifest: minimium_core::world::WorldManifest, output: &mut Vec<u8>) {
    output.extend_from_slice(&manifest.definition_id().get().to_le_bytes());
    output.push(match manifest.topology() {
        minimium_core::world::WorldTopology::Planet => 0,
        minimium_core::world::WorldTopology::Dungeon => 1,
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
    } else {
        output.push(0);
    }
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

pub(crate) fn write_position_mm(position: minimium_core::world::Position3Mm, output: &mut Vec<u8>) {
    for coordinate in position.0 {
        output.extend_from_slice(&coordinate.to_le_bytes());
    }
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
