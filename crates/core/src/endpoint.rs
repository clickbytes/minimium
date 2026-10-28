use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct ServerEndpoint {
    pub game_port: u16,
    pub admin_port: u16,
    pub token: String,
}

impl ServerEndpoint {
    pub fn file_path(data_dir: &Path) -> PathBuf {
        data_dir.join("server.endpoint")
    }

    pub fn write(data_dir: &Path, endpoint: &Self) -> std::io::Result<()> {
        fs::create_dir_all(data_dir)?;
        let path = Self::file_path(data_dir);
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&path)?;
        writeln!(file, "{}", endpoint.game_port)?;
        writeln!(file, "{}", endpoint.admin_port)?;
        writeln!(file, "{}", endpoint.token)?;
        file.sync_all()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    pub fn read(data_dir: &Path) -> std::io::Result<Self> {
        let mut content = String::new();
        File::open(Self::file_path(data_dir))?.read_to_string(&mut content)?;
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
        if token.len() != 64 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(invalid_endpoint());
        }
        Ok(Self {
            game_port,
            admin_port,
            token,
        })
    }
}

fn invalid_endpoint() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        "invalid server endpoint file",
    )
}

pub fn new_admin_token() -> std::io::Result<String> {
    let bytes = new_random_bytes::<32>()?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

pub fn new_random_bytes<const N: usize>() -> std::io::Result<[u8; N]> {
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
            return Err(std::io::Error::other(
                "operating system random generator failed",
            ));
        }
    }
    #[cfg(not(any(unix, windows)))]
    return Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "secure random source unavailable on this platform",
    ));
    Ok(bytes)
}
