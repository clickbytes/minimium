use super::common::{write_chat_text, write_name, write_vector, write_world_context};
use crate::{
    framing::encode_frame,
    kinds::ClientKind,
    models::{ClientMessage, ClientWorldInput, ExcavationInput, ProtocolError},
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
        ClientMessage::Authenticate {
            provider_id,
            credentials,
        } => {
            if provider_id.is_empty()
                || provider_id.len() > crate::limits::MAX_AUTH_PROVIDER_BYTES
                || !provider_id.bytes().all(|byte| {
                    byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
                })
                || credentials.len() > crate::limits::MAX_AUTH_CREDENTIAL_BYTES
            {
                return Err(ProtocolError::InvalidPayload);
            }
            payload.push(provider_id.len() as u8);
            payload.extend_from_slice(provider_id.as_bytes());
            payload.extend_from_slice(&(credentials.len() as u16).to_le_bytes());
            payload.extend_from_slice(credentials);
            ClientKind::Authenticate
        }
        ClientMessage::Ping => ClientKind::Ping,
        ClientMessage::SkyView {
            context,
            direction,
            observation,
        } => {
            write_world_context(*context, &mut payload);
            write_unit_direction(*direction, &mut payload)?;
            if !observation.valid() {
                return Err(ProtocolError::InvalidPayload);
            }
            super::server::write_position_mm(observation.position_mm, &mut payload);
            payload.extend_from_slice(&observation.calendar_epoch.get().to_le_bytes());
            payload.extend_from_slice(&observation.utc.get().to_le_bytes());
            payload.extend_from_slice(
                &observation
                    .night_until
                    .map_or(0, |end| end.get())
                    .to_le_bytes(),
            );
            ClientKind::SkyView
        }
        ClientMessage::WorldInput {
            sequence,
            control_revision,
            context,
            input,
        } => {
            if *sequence == 0 || *control_revision == 0 {
                return Err(ProtocolError::InvalidPayload);
            }
            write_world_context(*context, &mut payload);
            payload.extend_from_slice(&sequence.to_le_bytes());
            payload.extend_from_slice(&control_revision.to_le_bytes());
            match input {
                ClientWorldInput::Motion(sample) => {
                    payload.push(7);
                    if sample.input_seq == 0 || sample.target_tick == 0 {
                        return Err(ProtocolError::InvalidPayload);
                    }
                    payload.extend_from_slice(&sample.input_seq.to_le_bytes());
                    payload.extend_from_slice(&sample.target_tick.to_le_bytes());
                    crate::movement_codec::write(sample.intent, &mut payload)?;
                }
                ClientWorldInput::SetName(name) => {
                    payload.push(1);
                    write_name(name, &mut payload)?;
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
                ClientWorldInput::MovementPatch {
                    patch,
                    expected_revision,
                } => {
                    payload.push(13);
                    crate::policy::write_patch(&mut payload, *patch, *expected_revision)?;
                }
                ClientWorldInput::SetMovementPolicy { speed, accel } => {
                    let policy = minimium_simulation::MovementPolicy {
                        speed: *speed,
                        accel: *accel,
                        ..Default::default()
                    };
                    if !policy.valid() {
                        return Err(ProtocolError::InvalidPayload);
                    }
                    payload.push(9);
                    payload.extend_from_slice(&speed.to_le_bytes());
                    payload.extend_from_slice(&accel.to_le_bytes());
                }
                ClientWorldInput::SetFlightEnabled(enabled) => {
                    payload.push(10);
                    payload.push(u8::from(*enabled));
                }
                ClientWorldInput::InteractEntity { entity } => {
                    payload.push(11);
                    payload.extend_from_slice(&entity.get().to_le_bytes());
                }
                ClientWorldInput::MapTeleport { direction } => {
                    payload.push(6);
                    write_unit_direction(*direction, &mut payload)?;
                }
                ClientWorldInput::Excavation(input) => {
                    payload.push(12);
                    match input {
                        ExcavationInput::GrantTools => payload.push(1),
                        ExcavationInput::Equip { slot } => {
                            payload.push(2);
                            match slot {
                                None => payload.push(0),
                                Some(slot) if *slot < 2 => {
                                    payload.push(1);
                                    payload.push(*slot);
                                }
                                _ => return Err(ProtocolError::InvalidPayload),
                            }
                        }
                        ExcavationInput::Use {
                            action_id,
                            direction,
                        } => {
                            if *action_id == 0 {
                                return Err(ProtocolError::InvalidPayload);
                            }
                            payload.push(3);
                            payload.extend_from_slice(&action_id.to_le_bytes());
                            write_unit_direction(*direction, &mut payload)?;
                        }
                        ExcavationInput::Resync { coordinates } => {
                            if coordinates.iter().any(|v| !(-625_000..625_000).contains(v)) {
                                return Err(ProtocolError::InvalidPayload);
                            }
                            payload.push(4);
                            for coordinate in coordinates {
                                payload.extend_from_slice(&coordinate.to_le_bytes());
                            }
                        }
                    }
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
        ClientMessage::ReplicationAck {
            context,
            sync_id,
            revision,
            accepted,
        } => {
            if *sync_id == 0 {
                return Err(ProtocolError::InvalidPayload);
            }
            write_world_context(*context, &mut payload);
            payload.extend_from_slice(&sync_id.to_le_bytes());
            payload.extend_from_slice(&revision.to_le_bytes());
            payload.push(u8::from(*accepted));
            ClientKind::ReplicationAck
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
