use crate::endpoint::new_random_bytes;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub const PLAYER_PROFILE_ID_BYTES: usize = 16;
static NEXT_TEMP_FILE_ID: AtomicU64 = AtomicU64::new(0);

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

        // Serialize first-time creation while leaving the profile path absent
        // until a complete file is ready to publish.
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(data_dir.join("player.profile.lock"))?;
        lock.lock()?;
        match Self::read(&path) {
            Ok(identity) => return Ok(identity),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }

        let identity = Self(new_random_bytes()?);
        let (temporary_path, mut file) = create_temporary_profile_file(data_dir)?;
        let mut temporary_file = TemporaryProfileFile::new(temporary_path);
        writeln!(file, "MINIMIUM_PLAYER 1")?;
        writeln!(file, "{}", identity.to_hex())?;
        file.sync_all()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&temporary_file.path, fs::Permissions::from_mode(0o600))?;
        }
        drop(file);

        crate::files::atomic_replace_file(&temporary_file.path, &path)?;
        temporary_file.disarm();
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

struct TemporaryProfileFile {
    path: PathBuf,
    armed: bool,
}

impl TemporaryProfileFile {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
        let _ = fs::remove_file(&self.path);
    }
}

impl Drop for TemporaryProfileFile {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn create_temporary_profile_file(data_dir: &Path) -> io::Result<(PathBuf, File)> {
    loop {
        let id = NEXT_TEMP_FILE_ID.fetch_add(1, Ordering::Relaxed);
        let path = data_dir.join(format!(".player.profile.{}.{}.tmp", std::process::id(), id));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
}

fn invalid_profile_id() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid player profile file")
}
