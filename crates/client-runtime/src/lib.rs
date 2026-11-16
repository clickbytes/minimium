//! UI-independent client session and environment synchronization runtime.
mod authentication;
pub mod bootstrap;
mod connection_controller;
pub mod connector;
mod controller_error;
mod diagnostics;
pub mod environment;
pub mod excavation;
mod options;
mod prediction;
mod replicas;
mod session;
mod snapshots;
pub mod view;
mod water_replica;
mod world_content;

pub use authentication::{ClientAuthentication, ClientAuthenticationError};
pub use bootstrap::LocalServerLaunchConfig;
pub use connection_controller::{ConnectionController, ConnectionEvent};
pub use controller_error::ControllerError;
pub use options::ThreadLane;
pub use prediction::{ClientPrediction, PredictedStep};
pub use replicas::{ReplicaComponent, ReplicaEntity, ReplicaError, ReplicatedEntities};
pub use session::{
    ClientSession, DisconnectReason, QueueError, SessionEvent, SessionFailure, SessionIoFailure,
    SessionMetrics, SessionState, TransferTicket, HEARTBEAT_INTERVAL, HEARTBEAT_TIMEOUT,
    MAX_SESSION_BUFFER_BYTES, MAX_SESSION_READ_BYTES_PER_POLL, WELCOME_TIMEOUT,
};
pub use snapshots::{RemotePlayerTrack, SnapshotReconciler};
pub use view::ClientView;
pub use water_replica::WaterReplica;
pub use world_content::{
    ContentCancellation, ContentResolutionTicket, ResolvedWorldContent, WorldContentRequest,
    WorldContentResolver,
};
pub mod impacts;
