use crate::endpoint::new_random_bytes;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

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

    pub fn load_or_create(data_dir: &Path) -> io::Result<Self> {
        fs::create_dir_all(data_dir)?;
        let path = Self::file_path(data_dir);
        match Self::read(&path) {
            Ok(identity) => return Ok(identity),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }

        let identity = Self(new_random_bytes()?);
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return Self::read(&path),
            Err(error) => return Err(error),
        };
        let result = (|| {
            writeln!(file, "MINIMIUM_PLAYER 1")?;
            writeln!(file, "{}", identity.to_hex())?;
            file.sync_all()?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            let _ = fs::remove_file(path);
            return Err(error);
        }
        Ok(identity)
    }

    pub fn file_path(data_dir: &Path) -> PathBuf {
        data_dir.join("player.profile")
    }

    fn read(path: &Path) -> io::Result<Self> {
        let mut content = String::new();
        File::open(path)?.take(129).read_to_string(&mut content)?;
        if content.len() > 128 {
            return Err(invalid_profile_id());
        }
        let mut lines = content.lines();
        if lines.next() != Some("MINIMIUM_PLAYER 1") {
            return Err(invalid_profile_id());
        }
        let identity = Self::from_hex(lines.next().ok_or_else(invalid_profile_id)?)?;
        if lines.next().is_some() {
            return Err(invalid_profile_id());
        }
        Ok(identity)
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
