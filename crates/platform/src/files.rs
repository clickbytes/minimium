#[cfg(not(windows))]
use std::fs;
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    path::Path,
};

/// Opens and locks a coordination file. The lock is released when the file is
/// dropped, including when the owning process exits unexpectedly.
pub fn lock_file(path: &Path, exclusive: bool) -> io::Result<File> {
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(path)?;
    if exclusive {
        fs2::FileExt::lock_exclusive(&file)?;
    } else {
        fs2::FileExt::lock_shared(&file)?;
    }
    Ok(file)
}

/// Attempts to acquire an exclusive lock without waiting.
pub fn try_lock_file(path: &Path) -> io::Result<Option<File>> {
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(path)?;
    match fs2::FileExt::try_lock_exclusive(&file) {
        Ok(()) => Ok(Some(file)),
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(None),
        Err(error) => Err(error),
    }
}

/// Downgrades an exclusive cache update lock to a shared active-use lease.
pub fn downgrade_lock(file: File) -> io::Result<File> {
    fs2::FileExt::unlock(&file)?;
    fs2::FileExt::lock_shared(&file)?;
    Ok(file)
}

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
            io::ErrorKind::FileTooLarge,
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
    atomic_write_with(temporary_path, destination, |writer| {
        writer.write_all(bytes)
    })
}

/// Streams a bounded-format payload to a temporary file and atomically replaces the destination.
pub fn atomic_write_with(
    temporary_path: &Path,
    destination: &Path,
    write_payload: impl FnOnce(&mut File) -> io::Result<()>,
) -> io::Result<()> {
    let result = (|| {
        let mut file = File::create(temporary_path)?;
        write_payload(&mut file)?;
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
