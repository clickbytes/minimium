use super::{
    limits::{MAX_CHAT_BYTES, MAX_PLAYER_NAME_BYTES, PLAYER_PROFILE_ID_BYTES},
    models::ProtocolError,
};
use std::sync::Arc;

pub(crate) struct Reader<'a> {
    payload: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(payload: &'a [u8]) -> Self {
        Self { payload, offset: 0 }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], ProtocolError> {
        let end = self
            .offset
            .checked_add(count)
            .filter(|end| *end <= self.payload.len())
            .ok_or(ProtocolError::InvalidPayload)?;
        let bytes = &self.payload[self.offset..end];
        self.offset = end;
        Ok(bytes)
    }

    pub(crate) fn byte(&mut self) -> Result<u8, ProtocolError> {
        Ok(self.take(1)?[0])
    }

    pub(crate) fn boolean(&mut self) -> Result<bool, ProtocolError> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(ProtocolError::InvalidPayload),
        }
    }

    pub(crate) fn u64(&mut self) -> Result<u64, ProtocolError> {
        let bytes: [u8; 8] = self
            .take(8)?
            .try_into()
            .map_err(|_| ProtocolError::InvalidPayload)?;
        Ok(u64::from_le_bytes(bytes))
    }

    pub(crate) fn u16(&mut self) -> Result<u16, ProtocolError> {
        let bytes: [u8; 2] = self
            .take(2)?
            .try_into()
            .map_err(|_| ProtocolError::InvalidPayload)?;
        Ok(u16::from_le_bytes(bytes))
    }

    pub(crate) fn vector(&mut self) -> Result<[f32; 3], ProtocolError> {
        let mut vector = [0.0; 3];
        for value in &mut vector {
            let bytes: [u8; 4] = self
                .take(4)?
                .try_into()
                .map_err(|_| ProtocolError::InvalidPayload)?;
            *value = f32::from_le_bytes(bytes);
        }
        if !vector.iter().all(|value| value.is_finite()) {
            return Err(ProtocolError::InvalidPayload);
        }
        Ok(vector)
    }

    pub(crate) fn direction(&mut self) -> Result<[f32; 2], ProtocolError> {
        let mut direction = [0.0; 2];
        for value in &mut direction {
            let bytes: [u8; 4] = self
                .take(4)?
                .try_into()
                .map_err(|_| ProtocolError::InvalidPayload)?;
            *value = f32::from_le_bytes(bytes);
            if !value.is_finite() || !(-1.0..=1.0).contains(value) {
                return Err(ProtocolError::InvalidPayload);
            }
        }
        Ok(direction)
    }

    pub(crate) fn profile_id(&mut self) -> Result<[u8; PLAYER_PROFILE_ID_BYTES], ProtocolError> {
        self.take(PLAYER_PROFILE_ID_BYTES)?
            .try_into()
            .map_err(|_| ProtocolError::InvalidPayload)
    }

    pub(crate) fn name(&mut self) -> Result<String, ProtocolError> {
        Ok(self.name_text()?.to_owned())
    }

    pub(crate) fn shared_name(&mut self) -> Result<Arc<str>, ProtocolError> {
        Ok(Arc::from(self.name_text()?))
    }

    pub(crate) fn name_text(&mut self) -> Result<&'a str, ProtocolError> {
        let byte_count = self.byte()? as usize;
        let bytes = self.take(byte_count)?;
        let name = std::str::from_utf8(bytes).map_err(|_| ProtocolError::InvalidName)?;
        if name.is_empty()
            || name.len() > MAX_PLAYER_NAME_BYTES
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(ProtocolError::InvalidName);
        }
        Ok(name)
    }

    pub(crate) fn chat_text(&mut self) -> Result<&'a str, ProtocolError> {
        let byte_count = self.byte()? as usize;
        if byte_count == 0 || byte_count > MAX_CHAT_BYTES {
            return Err(ProtocolError::InvalidPayload);
        }
        let bytes = self.take(byte_count)?;
        let text = std::str::from_utf8(bytes).map_err(|_| ProtocolError::InvalidPayload)?;
        if text.chars().any(char::is_control) {
            return Err(ProtocolError::InvalidPayload);
        }
        Ok(text)
    }

    pub(crate) fn finish(&self) -> Result<(), ProtocolError> {
        if self.offset == self.payload.len() {
            Ok(())
        } else {
            Err(ProtocolError::InvalidPayload)
        }
    }
}
