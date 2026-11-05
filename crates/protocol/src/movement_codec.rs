use super::{reading::Reader, ProtocolError};
use minimium_simulation::{CharacterInput, MovementSettings};

pub(crate) fn write(input: CharacterInput, bytes: &mut Vec<u8>) -> Result<(), ProtocolError> {
    if !input.movement.valid() || !input.vertical.is_finite() || input.vertical.abs() > 1.0 {
        return Err(ProtocolError::InvalidPayload);
    }
    super::encoding::write_direction(input.direction, bytes)?;
    bytes.push(u8::from(input.jump));
    bytes.push(u8::from(input.movement.fly));
    for value in [input.vertical, input.movement.speed, input.movement.accel] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    Ok(())
}

pub(crate) fn read(reader: &mut Reader<'_>) -> Result<CharacterInput, ProtocolError> {
    let direction = reader.direction()?;
    let jump = reader.boolean()?;
    let fly = reader.boolean()?;
    let vertical = f32::from_le_bytes(reader.fixed()?);
    let speed = f32::from_le_bytes(reader.fixed()?);
    let accel = f32::from_le_bytes(reader.fixed()?);
    let movement = MovementSettings { fly, speed, accel };
    if !movement.valid() || !vertical.is_finite() || vertical.abs() > 1.0 {
        return Err(ProtocolError::InvalidPayload);
    }
    Ok(CharacterInput {
        direction,
        jump,
        vertical,
        movement,
    })
}
