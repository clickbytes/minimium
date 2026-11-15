use std::{error::Error, fmt, io};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageErrorKind {
    NotFound,
    InvalidFormat,
    UnsupportedVersion,
    LimitExceeded,
    Busy,
    WorkerStopped,
    Timeout,
    Io,
}

#[derive(Debug)]
pub struct StorageError {
    kind: StorageErrorKind,
    source: io::Error,
}

impl StorageError {
    pub fn kind(&self) -> StorageErrorKind {
        self.kind
    }
    pub fn code(&self) -> &'static str {
        match self.kind {
            StorageErrorKind::NotFound => "storage.not_found",
            StorageErrorKind::InvalidFormat => "storage.invalid_format",
            StorageErrorKind::UnsupportedVersion => "storage.unsupported_version",
            StorageErrorKind::LimitExceeded => "storage.limit_exceeded",
            StorageErrorKind::Busy => "storage.busy",
            StorageErrorKind::WorkerStopped => "storage.worker_stopped",
            StorageErrorKind::Timeout => "storage.shutdown_timeout",
            StorageErrorKind::Io => "storage.io_failed",
        }
    }
    pub(crate) fn from_io(source: io::Error) -> Self {
        let kind = match source.kind() {
            io::ErrorKind::NotFound => StorageErrorKind::NotFound,
            io::ErrorKind::InvalidData => StorageErrorKind::InvalidFormat,
            io::ErrorKind::Unsupported => StorageErrorKind::UnsupportedVersion,
            io::ErrorKind::FileTooLarge => StorageErrorKind::LimitExceeded,
            io::ErrorKind::WouldBlock => StorageErrorKind::Busy,
            io::ErrorKind::BrokenPipe => StorageErrorKind::WorkerStopped,
            io::ErrorKind::TimedOut => StorageErrorKind::Timeout,
            _ => StorageErrorKind::Io,
        };
        Self { kind, source }
    }
    pub(crate) fn busy(message: &'static str) -> Self {
        Self {
            kind: StorageErrorKind::Busy,
            source: io::Error::new(io::ErrorKind::WouldBlock, message),
        }
    }
    pub(crate) fn worker_stopped(message: &'static str) -> Self {
        Self {
            kind: StorageErrorKind::WorkerStopped,
            source: io::Error::new(io::ErrorKind::BrokenPipe, message),
        }
    }
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code(), self.source)
    }
}
impl Error for StorageError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.source)
    }
}
impl From<io::Error> for StorageError {
    fn from(source: io::Error) -> Self {
        Self::from_io(source)
    }
}
impl From<StorageError> for io::Error {
    fn from(error: StorageError) -> Self {
        io::Error::other(error)
    }
}
