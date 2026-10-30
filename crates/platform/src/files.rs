#[cfg(not(windows))]
use std::fs;
use std::{
    fs::File,
    io::{self, Read, Write},
    path::Path,
};

/// Default server data directory used by the server and its control client.
pub const DEFAULT_SERVER_DATA_DIR: &str = ".minimium";

/// Reads a persistent file and rejects its contents when they exceed the limit.
pub fn read_bounded_file(path: &Path, maximum_bytes: usize) -> io::Result<Vec<u8>> {
    let limit = u64::try_from(maximum_bytes)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    let mut bytes = Vec::with_capacity(maximum_bytes.min(4096));
    File::open(path)?.take(limit).read_to_end(&mut bytes)?;
    if bytes.len() > maximum_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "file exceeds size limit",
        ));
    }
    Ok(bytes)
}

/// Writes a payload and atomically replaces its destination on success.
pub fn atomic_write_file(
    temporary_path: &Path,
    destination: &Path,
    bytes: &[u8],
) -> io::Result<()> {
    let result = (|| {
        let mut file = File::create(temporary_path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        atomic_replace_file(temporary_path, destination)?;
        #[cfg(unix)]
        File::open(
            destination
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new(".")),
        )?
        .sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary_path);
    }
    result
}

#[cfg(windows)]
pub fn atomic_replace_file(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(source: *const u16, destination: *const u16, flags: u32) -> i32;
    }

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    let source: Vec<u16> = source.as_os_str().encode_wide().chain([0]).collect();
    let destination: Vec<u16> = destination.as_os_str().encode_wide().chain([0]).collect();
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
pub fn atomic_replace_file(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(test)]
mod tests;
