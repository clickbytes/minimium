use crate::{
    framing::decode_frame,
    kinds::ClientKind,
    models::{ClientMessage, ClientWorldInput, ProtocolError},
    reading::Reader,
    validation::is_unit_direction,
};
use minimium_world::entity::EntityId;

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
        ClientKind::Authenticate => ClientMessage::Authenticate {
            provider_id: reader.auth_provider()?,
            credentials: reader.auth_credentials()?,
        },
        ClientKind::Ping => ClientMessage::Ping,
        ClientKind::SkyView => {
            let context = reader.world_context()?;
            let direction = reader.vector()?;
            if !is_unit_direction(direction) {
                return Err(ProtocolError::InvalidPayload);
            }
            ClientMessage::SkyView {
                context,
                direction,
                night: reader.boolean()?,
            }
        }
        ClientKind::WorldInput => {
            let context = reader.world_context()?;
            let input = match reader.byte()? {
                1 => ClientWorldInput::SetName(reader.name()?),
                3 => {
                    let direction = reader.vector()?;
                    if !is_unit_direction(direction) {
                        return Err(ProtocolError::InvalidPayload);
                    }
                    ClientWorldInput::Fire { direction }
                }
                4 => ClientWorldInput::Chat(reader.chat_text()?.to_owned()),
                11 => ClientWorldInput::InteractEntity {
                    entity: EntityId::new(reader.u64()?).ok_or(ProtocolError::InvalidPayload)?,
                },
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
                9 => {
                    let speed = f32::from_le_bytes(reader.fixed()?);
                    let accel = f32::from_le_bytes(reader.fixed()?);
                    let policy = minimium_simulation::MovementPolicy {
                        speed,
                        accel,
                        ..Default::default()
                    };
                    if !policy.valid() {
                        return Err(ProtocolError::InvalidPayload);
                    }
                    ClientWorldInput::SetMovementPolicy { speed, accel }
                }
                10 => ClientWorldInput::SetFlightEnabled(reader.boolean()?),
                7 => {
                    let input_seq = reader.u64()?;
                    let target_tick = reader.u64()?;
                    if input_seq == 0 || target_tick == 0 {
                        return Err(ProtocolError::InvalidPayload);
                    }
                    ClientWorldInput::Motion(crate::models::MovementSample {
                        input_seq,
                        target_tick,
                        intent: crate::movement_codec::read(&mut reader)?,
                    })
                }
                _ => return Err(ProtocolError::InvalidPayload),
            };
            ClientMessage::WorldInput { context, input }
        }
        ClientKind::TransitionReady => ClientMessage::TransitionReady {
            transition_id: reader.transition_id()?,
            target_context: reader.world_context()?,
        },
        ClientKind::CancelTransition => ClientMessage::CancelTransition(reader.transition_id()?),
        ClientKind::TimeProbe => ClientMessage::TimeProbe {
            seq: reader.u32()?,
            client_send_monotonic_ms: reader.u64()?,
        },
        ClientKind::ReplicationAck => {
            let context = reader.world_context()?;
            let sync_id = reader.u64()?;
            if sync_id == 0 {
                return Err(ProtocolError::InvalidPayload);
            }
            ClientMessage::ReplicationAck {
                context,
                sync_id,
                revision: reader.u64()?,
                accepted: reader.boolean()?,
            }
        }
        ClientKind::Quit => ClientMessage::Quit,
    };
    reader.finish()?;
    Ok(Some((message, frame_bytes)))
}
