use crate::{
    models::ProtocolError,
    validation::{valid_chat, valid_name},
};

pub(crate) fn write_name(name: &str, output: &mut Vec<u8>) -> Result<(), ProtocolError> {
    if !valid_name(name) {
        return Err(ProtocolError::InvalidName);
    }
    output.push(name.len() as u8);
    output.extend_from_slice(name.as_bytes());
    Ok(())
}

pub(super) fn write_world_context(
    context: crate::world_context::WorldContext,
    output: &mut Vec<u8>,
) {
    output.extend_from_slice(&context.instance().get().to_le_bytes());
    output.extend_from_slice(&context.content_revision().get().to_le_bytes());
    output.extend_from_slice(&context.membership_epoch().get().to_le_bytes());
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
