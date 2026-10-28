use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const MAX_ENDPOINT_BYTES: usize = 256;
static NEXT_TEMP_FILE_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
pub struct ServerEndpoint {
    pub game_port: u16,
    pub admin_port: u16,
    pub token: String,
}

impl fmt::Debug for ServerEndpoint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ServerEndpoint")
            .field("game_port", &self.game_port)
            .field("admin_port", &self.admin_port)
            .field("token", &"[REDACTED]")
            .finish()
    }
}

impl ServerEndpoint {
    pub fn file_path(data_dir: &Path) -> PathBuf {
        data_dir.join("server.endpoint")
    }

    pub fn write(data_dir: &Path, endpoint: &Self) -> io::Result<()> {
        if !valid_token(&endpoint.token) {
            return Err(invalid_endpoint());
        }
        fs::create_dir_all(data_dir)?;
        let destination = Self::file_path(data_dir);
        let (temporary_path, mut file) = create_temporary_endpoint_file(data_dir)?;
        let mut temporary_file = TemporaryEndpointFile::new(temporary_path);

        writeln!(file, "{}", endpoint.game_port)?;
        writeln!(file, "{}", endpoint.admin_port)?;
        writeln!(file, "{}", endpoint.token)?;
        file.sync_all()?;
        drop(file);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&temporary_file.path, fs::Permissions::from_mode(0o600))?;
        }

        crate::files::atomic_replace_file(&temporary_file.path, &destination)?;
        temporary_file.disarm();
        Ok(())
    }

    pub fn read(data_dir: &Path) -> io::Result<Self> {
        let mut bytes = Vec::with_capacity(MAX_ENDPOINT_BYTES + 1);
        File::open(Self::file_path(data_dir))?
            .take((MAX_ENDPOINT_BYTES + 1) as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len() > MAX_ENDPOINT_BYTES {
            return Err(invalid_endpoint());
        }
        let content = String::from_utf8(bytes).map_err(|_| invalid_endpoint())?;
        let mut lines = content.lines();
        let game_port = lines
            .next()
            .ok_or_else(invalid_endpoint)?
            .parse()
            .map_err(|_| invalid_endpoint())?;
        let admin_port = lines
            .next()
            .ok_or_else(invalid_endpoint)?
            .parse()
            .map_err(|_| invalid_endpoint())?;
        let token = lines.next().ok_or_else(invalid_endpoint)?.to_string();
        if lines.next().is_some() || !valid_token(&token) {
            return Err(invalid_endpoint());
        }
        Ok(Self {
            game_port,
            admin_port,
            token,
        })
    }

    pub fn remove_if_owned(&self, data_dir: &Path) -> io::Result<()> {
        let path = Self::file_path(data_dir);
        let current = match Self::read(data_dir) {
            Ok(current) => current,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::InvalidData => return Ok(()),
            Err(error) => return Err(error),
        };
        if current.game_port == self.game_port
            && current.admin_port == self.admin_port
            && current.token == self.token
        {
            match fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error),
            }
        } else {
            Ok(())
        }
    }
}

struct TemporaryEndpointFile {
    path: PathBuf,
    armed: bool,
}

impl TemporaryEndpointFile {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TemporaryEndpointFile {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn create_temporary_endpoint_file(data_dir: &Path) -> io::Result<(PathBuf, File)> {
    loop {
        let id = NEXT_TEMP_FILE_ID.fetch_add(1, Ordering::Relaxed);
        let path = data_dir.join(format!(
            ".server.endpoint.{}.{}.tmp",
            std::process::id(),
            id
        ));
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

fn invalid_endpoint() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid server endpoint file")
}

fn valid_token(token: &str) -> bool {
    token.len() == 64 && token.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn new_admin_token() -> io::Result<String> {
    let bytes = new_random_bytes::<32>()?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

pub fn new_random_bytes<const N: usize>() -> io::Result<[u8; N]> {
    let mut bytes = [0u8; N];
    #[cfg(unix)]
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    #[cfg(windows)]
    {
        #[link(name = "bcrypt")]
        unsafe extern "system" {
            fn BCryptGenRandom(
                algorithm: *mut std::ffi::c_void,
                buffer: *mut u8,
                length: u32,
                flags: u32,
            ) -> i32;
        }
        const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 0x00000002;
        let result = unsafe {
            BCryptGenRandom(
                std::ptr::null_mut(),
                bytes.as_mut_ptr(),
                bytes.len() as u32,
                BCRYPT_USE_SYSTEM_PREFERRED_RNG,
            )
        };
        if result < 0 {
            return Err(io::Error::other("operating system random generator failed"));
        }
    }
    #[cfg(not(any(unix, windows)))]
    return Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "secure random source unavailable on this platform",
    ));
    Ok(bytes)
}
