//! UI-independent client session and environment synchronization runtime.
pub mod bootstrap;
mod connection_controller;
pub mod connector;
pub mod environment;
mod options;
mod prediction;
mod session;
mod snapshots;
pub mod view;

pub use bootstrap::LocalServerLaunchConfig;
pub use connection_controller::{ConnectionController, ConnectionEvent};
pub use options::ThreadLane;
pub use prediction::{ClientPrediction, PredictedStep};
pub use session::{
    ClientSession, DisconnectReason, QueueError, SessionEvent, SessionMetrics, SessionState,
    HEARTBEAT_INTERVAL, HEARTBEAT_TIMEOUT, MAX_SESSION_BUFFER_BYTES,
    MAX_SESSION_READ_BYTES_PER_POLL, WELCOME_TIMEOUT,
};
pub use snapshots::{RemotePlayerTrack, SnapshotReconciler};
pub use view::ClientView;
