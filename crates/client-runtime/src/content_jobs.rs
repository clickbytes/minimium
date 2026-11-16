//! Host-executed, bounded content work. The session owns admission and publication.
use crate::{
    ContentCancellation, ContentResolutionTicket, ResolvedWorldContent, SessionFailure,
    WorldContentRequest,
};
use minimium_asset_io::materials::MaterialPackage;
use std::{net::SocketAddr, num::NonZeroU64, sync::Arc, time::Instant};

/// One material negotiation in one session lifetime.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MaterialContentTicket {
    pub session_id: u64,
    pub request_id: u64,
    pub manifest_hash: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ContentJobTicket {
    World(ContentResolutionTicket),
    Material(MaterialContentTicket),
}

/// A port-issued accepted ticket with its fresh, nonblocking cancellation intent.
/// Requesting cancellation never claims the work has already exited.
#[derive(Clone)]
pub struct ContentJobHandle {
    ticket: ContentJobTicket,
    cancellation: ContentCancellation,
}

impl ContentJobHandle {
    pub fn new(ticket: ContentJobTicket, cancellation: ContentCancellation) -> Self {
        Self {
            ticket,
            cancellation,
        }
    }
    pub const fn ticket(&self) -> ContentJobTicket {
        self.ticket
    }
    pub fn cancellation(&self) -> ContentCancellation {
        self.cancellation.clone()
    }
    pub fn request_cancel(&self) {
        self.cancellation.cancel();
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }
}

#[derive(Clone, Copy)]
pub struct MaterialContentRequest {
    pub ticket: MaterialContentTicket,
    pub endpoint: SocketAddr,
    pub compressed: bool,
}

#[derive(Clone, Copy)]
pub enum ContentJobRequest {
    World(WorldContentRequest),
    Material(MaterialContentRequest),
}

impl ContentJobRequest {
    pub const fn ticket(self) -> ContentJobTicket {
        match self {
            Self::World(request) => ContentJobTicket::World(request.ticket),
            Self::Material(request) => ContentJobTicket::Material(request.ticket),
        }
    }
}

/// A complete CPU bundle. Its catalog is prepared from the exact resolved source.
/// This pure constructor performs no thread, file or transport operations.
pub struct PreparedWorldContent {
    content: ResolvedWorldContent,
    water_catalog: Option<Arc<minimium_procgen::WaterCatalog>>,
}

impl PreparedWorldContent {
    pub fn prepare(content: ResolvedWorldContent) -> Result<Self, SessionFailure> {
        let ticket = content.ticket();
        let inputs = minimium_procgen::TerrainInputs::new(
            content.prepared_terrain().clone(),
            minimium_procgen::TerrainOverlaySnapshot::empty_for(ticket.context.instance()),
        )
        .map_err(|error| {
            SessionFailure::caused_by(
                error.code(),
                "world terrain source does not match its content ticket",
                error,
            )
            .with_content_ticket(ticket)
        })?;
        let water_catalog = if content.manifest().topology()
            == minimium_world::world::WorldTopology::Planet
        {
            let terrain = minimium_procgen::PlanetTerrain::for_manifest(content.manifest(), inputs)
                .map_err(|error| {
                    SessionFailure::caused_by(
                        error.code(),
                        "world water catalog preparation failed",
                        error,
                    )
                    .with_content_ticket(ticket)
                })?;
            Some(Arc::new(
                minimium_procgen::WaterCatalog::build(
                    &terrain,
                    minimium_procgen::DEFAULT_WATER_CATALOG_RESOLUTION,
                )
                .map_err(|error| {
                    SessionFailure::caused_by(
                        error.code(),
                        "world water catalog preparation failed",
                        error,
                    )
                    .with_content_ticket(ticket)
                })?,
            ))
        } else {
            None
        };
        Ok(Self {
            content,
            water_catalog,
        })
    }

    pub fn content(&self) -> &ResolvedWorldContent {
        &self.content
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        ResolvedWorldContent,
        Option<Arc<minimium_procgen::WaterCatalog>>,
    ) {
        (self.content, self.water_catalog)
    }
}

pub enum ContentJobResult {
    World(Result<PreparedWorldContent, SessionFailure>),
    Material(Result<MaterialPackage, SessionFailure>),
}

pub struct ContentJobCompletion {
    pub ticket: ContentJobTicket,
    pub result: ContentJobResult,
}

pub enum ContentJobPoll {
    Pending,
    Complete(ContentJobCompletion),
    /// Cancellation was requested earlier and the owned work has now exited.
    Cancelled,
    Unknown,
}

#[derive(Clone, Debug)]
pub enum ContentSubmitError {
    Busy,
    Closed,
    Rejected(SessionFailure),
}

impl std::fmt::Display for ContentSubmitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Busy => f.write_str("content service is busy"),
            Self::Closed => f.write_str("content service is closed"),
            Self::Rejected(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ContentSubmitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Rejected(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContentCancelStatus {
    Requested,
    Finished,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContentShutdownStatus {
    Complete,
    Pending,
}

/// A typed service, not an arbitrary-closure executor. Submit/poll/cancel never
/// wait for running work. Accepted jobs and undrained completions reserve bounded
/// capacity until consumed or confirmed cancelled. Cancellation is not completion.
pub trait ContentJobsPort: Send + Sync {
    /// Default host bounds are one world and one material slot, including ready
    /// completions and cancelled running work. Alternate ports must declare and
    /// enforce finite caps. Busy does not accept the request.
    fn submit(&self, request: ContentJobRequest) -> Result<ContentJobHandle, ContentSubmitError>;
    fn poll(&self, ticket: ContentJobTicket) -> ContentJobPoll;
    /// Record the handle's atomic intent before any optional cleanup. This must
    /// not wait for a mutex or worker; Requested is distinct from confirmed exit.
    fn cancel(&self, handle: &ContentJobHandle) -> ContentCancelStatus;
    /// Host shutdown may wait only until this absolute deadline. A Pending result
    /// means work still owns its resources; it does not promise I/O interruption.
    fn shutdown_until(&self, deadline: Instant) -> ContentShutdownStatus;
}

/// Explicit session identity over a host-owned service shared across reconnects.
/// The host must never reuse an ID during the shared service's lifetime, even
/// after completion: old handles may still exist and must not address new work.
pub struct ContentJobsSession {
    session_id: NonZeroU64,
    port: Arc<dyn ContentJobsPort>,
}

impl ContentJobsSession {
    pub fn new(session_id: NonZeroU64, port: Arc<dyn ContentJobsPort>) -> Self {
        Self { session_id, port }
    }
    pub const fn session_id(&self) -> u64 {
        self.session_id.get()
    }
    pub(crate) fn port(&self) -> &dyn ContentJobsPort {
        self.port.as_ref()
    }
}
