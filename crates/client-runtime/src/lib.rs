//! UI-independent client session and environment synchronization runtime.
// Shared example fixtures also compile inside the library test target.
#[cfg(test)]
extern crate self as minimium_client_runtime;

mod application;
mod authentication;
pub mod bootstrap;
mod client_time;
mod connection_controller;
pub mod connector;
mod content_jobs;
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
pub mod world_readiness;

pub use application::{
    ApplicationMessageError, AppliedSessionEvent, ClientAction, ClientApplicationConfig,
    ClientApplicationCore, ClientApplicationCreateError, ClientReadinessError,
    ClientStepDisconnect, ClientStepInput, ClientStepOutput, LocalActionOutcome, PredictionInput,
    PredictionPresentationRequest, PredictionSceneRequest, QueuedActionKind, ReplicaPlayerSample,
    SuppressionReason, UseTrigger, WorldContextChange,
};
pub use authentication::{ClientAuthentication, ClientAuthenticationError};
pub use bootstrap::LocalServerLaunchConfig;
pub use client_time::{ClientTime, ClientTimeError};
pub use connection_controller::{
    valid_endpoint, ConnectionController, ConnectionEvent, ConnectionStats, MovementField,
    PreparedSessionError, SessionTraffic,
};
pub use content_jobs::{
    ContentCancelStatus, ContentJobCompletion, ContentJobHandle, ContentJobPoll, ContentJobRequest,
    ContentJobResult, ContentJobTicket, ContentJobsPort, ContentJobsSession, ContentShutdownStatus,
    ContentSubmitError, MaterialContentRequest, MaterialContentTicket, PreparedWorldContent,
};
pub use controller_error::ControllerError;
pub use options::ThreadLane;
pub use prediction::{ClientPrediction, PredictedStep};
pub use replicas::{ReplicaComponent, ReplicaEntity, ReplicaError, ReplicatedEntities};
pub use session::{
    ClientSession, DisconnectReason, QueueError, QueuedWorldFrame, SessionEvent, SessionFailure,
    SessionIoFailure, SessionMetrics, SessionState, TransferTicket, WaterReplicaSummary,
    HEARTBEAT_INTERVAL, HEARTBEAT_TIMEOUT, MAX_SESSION_BUFFER_BYTES,
    MAX_SESSION_READ_BYTES_PER_POLL, WELCOME_TIMEOUT,
};
pub use snapshots::{RemotePlayerTrack, SnapshotReconciler};
pub use view::ClientView;
pub use water_replica::WaterReplica;
pub use world_content::{
    ContentCancellation, ContentResolutionTicket, ResolvedWorldContent, WorldContentRequest,
    WorldContentResolver,
};
pub mod impacts;
