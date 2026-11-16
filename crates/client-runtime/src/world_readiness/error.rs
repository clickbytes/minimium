use minimium_sectors::SectorTicket;
use std::{fmt, io};

#[derive(Debug)]
pub enum WorldReadinessError {
    ThreadSpawn(io::Error),
    QueueFull,
    CacheFull,
    WorkerStopped(Vec<SectorTicket>),
    Geometry(minimium_procgen::ExcavationGeometryError),
    InvalidRequest,
    RevisionExhausted,
}

impl WorldReadinessError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::RevisionExhausted => "client_runtime.collision_revision_exhausted",
            Self::InvalidRequest => "client_runtime.collision_request_invalid",
            Self::ThreadSpawn(_) => "client_runtime.sector_worker_start_failed",
            Self::QueueFull => "client_runtime.sector_worker_queue_full",
            Self::CacheFull => "client_runtime.sector_cache_full",
            Self::WorkerStopped(_) => "client_runtime.sector_worker_stopped",
            Self::Geometry(_) => "client_runtime.sector_geometry_failed",
        }
    }
}

impl fmt::Display for WorldReadinessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RevisionExhausted => formatter.write_str(self.code()),
            Self::ThreadSpawn(error) => write!(formatter, "{}: {error}", self.code()),
            Self::InvalidRequest => formatter.write_str(self.code()),
            Self::QueueFull => formatter.write_str(self.code()),
            Self::CacheFull => formatter.write_str(self.code()),
            Self::Geometry(error) => write!(formatter, "{}: {error:?}", self.code()),
            Self::WorkerStopped(tickets) => {
                write!(formatter, "{} ({} tickets)", self.code(), tickets.len())
            }
        }
    }
}

impl std::error::Error for WorldReadinessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ThreadSpawn(error) => Some(error),
            Self::QueueFull
            | Self::CacheFull
            | Self::WorkerStopped(_)
            | Self::Geometry(_)
            | Self::InvalidRequest
            | Self::RevisionExhausted => None,
        }
    }
}
