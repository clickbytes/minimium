use serde::{Deserialize, Serialize};
use std::{
    io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const STATUS_PREFIX: &str = ".server-startup-";
static NEXT_ATTEMPT: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupState {
    Starting,
    Ready,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ServerStartupStatus {
    pub attempt_id: String,
    pub state: StartupState,
    pub error_code: Option<String>,
    pub run_id: Option<String>,
}

impl ServerStartupStatus {
    pub fn new(
        attempt_id: String,
        state: StartupState,
        error_code: Option<String>,
        run_id: Option<String>,
    ) -> io::Result<Self> {
        if !valid_attempt_id(&attempt_id)
            || error_code.as_deref().is_some_and(|code| !valid_code(code))
            || run_id.as_deref().is_some_and(|run| !valid_run_id(run))
            || (state == StartupState::Failed) != error_code.is_some()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid server startup status fields",
            ));
        }
        Ok(Self {
            attempt_id,
            state,
            error_code,
            run_id,
        })
    }
}

pub fn new_attempt_id() -> io::Result<String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
        .as_nanos();
    let sequence = NEXT_ATTEMPT.fetch_add(1, Ordering::Relaxed);
    Ok(format!("{}-{nanos:x}-{sequence:x}", std::process::id()))
}

pub fn status_path(data_dir: &Path, attempt_id: &str) -> io::Result<PathBuf> {
    if !valid_attempt_id(attempt_id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid server startup attempt identifier",
        ));
    }
    Ok(data_dir.join(format!("{STATUS_PREFIX}{attempt_id}.json")))
}

pub fn write(data_dir: &Path, status: &ServerStartupStatus) -> io::Result<()> {
    std::fs::create_dir_all(data_dir)?;
    let path = status_path(data_dir, &status.attempt_id)?;
    let temporary = data_dir.join(format!("{STATUS_PREFIX}{}.tmp", status.attempt_id));
    let bytes = serde_json::to_vec(status).map_err(io::Error::other)?;
    super::files::atomic_write_file(&temporary, &path, &bytes)
}

pub fn read(data_dir: &Path, attempt_id: &str) -> io::Result<ServerStartupStatus> {
    let bytes = super::files::read_bounded_file(&status_path(data_dir, attempt_id)?, 4096)?;
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

fn valid_attempt_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn valid_code(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
}

fn valid_run_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'-' || byte == b'/')
        && value.contains('/')
}
