use super::{reading::Reader, ProtocolError};
use minimium_simulation::MovementIntent;

pub(crate) fn write(input: MovementIntent, bytes: &mut Vec<u8>) -> Result<(), ProtocolError> {
    if !input.vertical.is_finite() || input.vertical.abs() > 1.0 {
        return Err(ProtocolError::InvalidPayload);
    }
    super::encoding::write_direction(input.direction, bytes)?;
    bytes.push(u8::from(input.jump));
    bytes.extend_from_slice(&input.vertical.to_le_bytes());
    Ok(())
}

pub(crate) fn read(reader: &mut Reader<'_>) -> Result<MovementIntent, ProtocolError> {
    let direction = reader.direction()?;
    let jump = reader.boolean()?;
    let vertical = f32::from_le_bytes(reader.fixed()?);
    if !vertical.is_finite() || vertical.abs() > 1.0 {
        return Err(ProtocolError::InvalidPayload);
    }
    Ok(MovementIntent {
        direction,
        jump,
        vertical,
    })
}
