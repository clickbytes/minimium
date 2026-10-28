use super::{
    limits::{HEADER_BYTES, MAX_PAYLOAD_BYTES, PROTOCOL_VERSION},
    models::ProtocolError,
};

pub(crate) fn encode_frame(kind: u8, payload: &[u8]) -> Result<Vec<u8>, ProtocolError> {
    if payload.len() > MAX_PAYLOAD_BYTES || payload.len() > u16::MAX as usize {
        return Err(ProtocolError::PayloadTooLarge);
    }
    let mut frame = Vec::with_capacity(HEADER_BYTES + payload.len());
    frame.push(PROTOCOL_VERSION);
    frame.push(kind);
    frame.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    frame.extend_from_slice(payload);
    Ok(frame)
}

type DecodedFrame<'a> = (u8, &'a [u8], usize);

pub(crate) fn decode_frame(input: &[u8]) -> Result<Option<DecodedFrame<'_>>, ProtocolError> {
    if input.len() < HEADER_BYTES {
        return Ok(None);
    }
    if input[0] != PROTOCOL_VERSION {
        return Err(ProtocolError::UnsupportedVersion(input[0]));
    }
    let payload_bytes = u16::from_le_bytes([input[2], input[3]]) as usize;
    if payload_bytes > MAX_PAYLOAD_BYTES {
        return Err(ProtocolError::PayloadTooLarge);
    }
    let frame_bytes = HEADER_BYTES + payload_bytes;
    if input.len() < frame_bytes {
        return Ok(None);
    }
    Ok(Some((
        input[1],
        &input[HEADER_BYTES..frame_bytes],
        frame_bytes,
    )))
}
