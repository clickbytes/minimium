use std::{
    fs::{self, File, OpenOptions},
    io,
    path::Path,
};

/// Holds the exclusive lock for a server data directory for its lifetime.
///
/// The lock file is deliberately left in place when the handle is dropped;
/// operating-system file locking releases ownership automatically.
pub struct ServerDataLock {
    _file: File,
}

impl ServerDataLock {
    pub fn acquire(data_dir: &Path) -> io::Result<Self> {
        fs::create_dir_all(data_dir)?;
        let file = open_lock_file(data_dir)?;
        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file }),
            Err(std::fs::TryLockError::WouldBlock) => Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "server data directory is already locked",
            )),
            Err(std::fs::TryLockError::Error(error)) => Err(error),
        }
    }
}

/// Reports whether a server currently holds the data directory's exclusive lock.
///
/// This is an instantaneous observation; a server may acquire or release the
/// lock immediately after this function returns.
pub fn is_locked(data_dir: &Path) -> io::Result<bool> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(data_dir.join("server.lock"))?;
    match file.try_lock() {
        Ok(()) => Ok(false),
        Err(std::fs::TryLockError::WouldBlock) => Ok(true),
        Err(std::fs::TryLockError::Error(error)) => Err(error),
    }
}

fn open_lock_file(data_dir: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(data_dir.join("server.lock"))
}
