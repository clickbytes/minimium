use crate::endpoint::new_random_bytes;
use minimium_core::identity::PlayerProfileId;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_TEMP_FILE_ID: AtomicU64 = AtomicU64::new(0);

pub fn load_or_create(data_dir: &Path) -> io::Result<PlayerProfileId> {
    fs::create_dir_all(data_dir)?;
    let path = file_path(data_dir);
    match read(&path) {
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
    match read(&path) {
        Ok(identity) => return Ok(identity),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    let identity = PlayerProfileId::from_bytes(new_random_bytes()?);
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

fn file_path(data_dir: &Path) -> PathBuf {
    data_dir.join("player.profile")
}

fn read(path: &Path) -> io::Result<PlayerProfileId> {
    let mut content = String::new();
    File::open(path)?.take(129).read_to_string(&mut content)?;
    if content.len() > 128 {
        return Err(invalid_profile_id());
    }
    let mut lines = content.lines();
    if lines.next() != Some("MINIMIUM_PLAYER 1") {
        return Err(invalid_profile_id());
    }
    let identity = PlayerProfileId::from_hex(lines.next().ok_or_else(invalid_profile_id)?)?;
    if lines.next().is_some() {
        return Err(invalid_profile_id());
    }
    Ok(identity)
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
