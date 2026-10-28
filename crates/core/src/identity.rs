use std::io;

pub const PLAYER_PROFILE_ID_BYTES: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlayerProfileId([u8; PLAYER_PROFILE_ID_BYTES]);

impl PlayerProfileId {
    pub fn from_bytes(bytes: [u8; PLAYER_PROFILE_ID_BYTES]) -> Self {
        Self(bytes)
    }

    pub fn from_hex(value: &str) -> io::Result<Self> {
        if value.len() != PLAYER_PROFILE_ID_BYTES * 2 {
            return Err(invalid_profile_id());
        }
        let mut bytes = [0u8; PLAYER_PROFILE_ID_BYTES];
        for (index, byte) in bytes.iter_mut().enumerate() {
            let offset = index * 2;
            let high = hex_digit(value.as_bytes()[offset]).ok_or_else(invalid_profile_id)?;
            let low = hex_digit(value.as_bytes()[offset + 1]).ok_or_else(invalid_profile_id)?;
            *byte = (high << 4) | low;
        }
        Ok(Self(bytes))
    }

    pub fn as_bytes(&self) -> &[u8; PLAYER_PROFILE_ID_BYTES] {
        &self.0
    }

    pub fn to_hex(self) -> String {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut output = String::with_capacity(PLAYER_PROFILE_ID_BYTES * 2);
        for byte in self.0 {
            output.push(HEX[usize::from(byte >> 4)] as char);
            output.push(HEX[usize::from(byte & 0x0f)] as char);
        }
        output
    }
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn invalid_profile_id() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid player profile file")
}
