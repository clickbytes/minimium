//! Shared English display messages for the terminal and rendered clients.
use crate::{
    client_commands::ParseError,
    session::{DisconnectReason, QueueError},
};
use minimium_protocol::{ProtocolError, ServerErrorCode};
use std::fmt;

/// A display message with typed values supplied by the client event.
#[derive(Clone, Debug, PartialEq)]
pub enum ClientText {
    ConnectingTo(String),
    StartingLocalServer,
    BusyAttempt {
        attempt_id: u64,
    },
    AttemptIdsExhausted,
    WorkerStartFailed(String),
    AttemptTimedOut,
    WaitingForCancelledCleanup,
    AwaitingWelcome,
    ConnectionSetupFailed(String),
    ConnectFailed(String),
    DisconnectOrWait,
    Disconnected,
    NotConnected,
    NoServerResponse,
    CommandQueued,
    CouldNotQueueCommand(QueueProblem),
    CouldNotFire(QueueProblem),
    CommandParseError(ParseError),
    TerminalIntro,
    CommandLineHelp,
    InputPrompt,
    ClientError(String),
    MapLoadFailed {
        path: String,
        detail: String,
    },
    BundledMapInvalid(String),
    ConsoleInputQueueFull,
    ConsoleInputReadFailed(String),
    ConsoleInputTooLong {
        max_bytes: usize,
    },
    WindowCreationFailed(String),
    GraphicsInitializationFailed(String),
    RendererAlreadyInitialized,
    RenderFailed(String),
    WindowTitle,
    WindowTitleStatus {
        fps: f64,
        health: u16,
    },
    LocalServerGuardCleanupFailed(String),
    LocalServerCheckFailed(String),
    CouldNotQueueMovement(QueueProblem),
    FpsUnavailable,
    ConsoleHistoryHelp,
    ShotFired,
    VisiblePlayerLimitExceeded,
    TerminalSnapshot {
        server_time_ms: u64,
        full_snapshot: bool,
        self_position: [f32; 3],
        self_velocity: [f32; 3],
        grounded: bool,
        player_updates: usize,
        player_removals: usize,
    },
    PlayerState {
        position: [f32; 3],
        health: u16,
        nearby_players: usize,
    },
    WelcomeTimeout,
    HeartbeatTimeout,
    PeerDisconnected,
    ServerClosed,
    Rejected {
        code: ServerErrorCode,
    },
    ServerError {
        code: ServerErrorCode,
    },
    MapMismatch {
        expected: u64,
        actual: u64,
    },
    ConnectionLost(String),
    Connected {
        id: minimium_core::ids::PlayerId,
        health: u16,
    },
    Ping {
        milliseconds: u128,
    },
    NetworkStats {
        bytes_sent: u64,
        bytes_received: u64,
        ping_milliseconds: Option<u128>,
        nearby_players: Option<usize>,
    },
    Pong,
    Accepted,
    Health {
        health: u16,
    },
    ShotMissed,
    Hit {
        target_id: minimium_core::ids::PlayerId,
        health: u16,
    },
    TargetRespawned {
        target_id: minimium_core::ids::PlayerId,
        health: u16,
    },
    Chat {
        sender_name: String,
        sender_id: minimium_core::ids::PlayerId,
        text: String,
    },
}

/// Display-safe categories for session queue failures.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueueProblem {
    Disconnected,
    AwaitingWelcome,
    IdentifyReserved,
    StaleWorldContext,
    BufferFull,
    Protocol(ProtocolError),
}

impl From<QueueError> for QueueProblem {
    fn from(error: QueueError) -> Self {
        match error {
            QueueError::Disconnected => Self::Disconnected,
            QueueError::AwaitingWelcome => Self::AwaitingWelcome,
            QueueError::IdentifyReserved => Self::IdentifyReserved,
            QueueError::StaleWorldContext => Self::StaleWorldContext,
            QueueError::BufferFull => Self::BufferFull,
            QueueError::Protocol(error) => Self::Protocol(error),
        }
    }
}

impl From<DisconnectReason> for ClientText {
    fn from(reason: DisconnectReason) -> Self {
        match reason {
            DisconnectReason::WelcomeTimeout => Self::WelcomeTimeout,
            DisconnectReason::HeartbeatTimeout => Self::HeartbeatTimeout,
            DisconnectReason::Rejected(code) => Self::Rejected { code },
            DisconnectReason::MapMismatch { expected, actual } => {
                Self::MapMismatch { expected, actual }
            }
            DisconnectReason::PeerClosed => Self::PeerDisconnected,
            DisconnectReason::ServerClosed => Self::ServerClosed,
            DisconnectReason::EarthPackage(error) => Self::ConnectionLost(error),
            other => Self::ConnectionLost(format!("{other:?}")),
        }
    }
}

fn server_error_text(code: ServerErrorCode) -> &'static str {
    match code {
        ServerErrorCode::Protocol => "server rejected the protocol handshake",
        ServerErrorCode::Busy => "server is busy; this can include a duplicate active profile",
        ServerErrorCode::ServerFull => "server is full",
        ServerErrorCode::RateLimit => "request was rate-limited",
        _ => "server rejected the request",
    }
}

impl fmt::Display for QueueProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Disconnected => "not connected",
            Self::AwaitingWelcome => "waiting for server welcome",
            Self::IdentifyReserved => "identify is reserved for connection setup",
            Self::StaleWorldContext => "world changed; reconnect or retry the command",
            Self::BufferFull => "outgoing queue is full",
            Self::Protocol(error) => return write!(f, "message could not be encoded: {error}"),
        })
    }
}

impl fmt::Display for ClientText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConnectingTo(address) => write!(f, "connecting to {address}..."),
            Self::StartingLocalServer => f.write_str("starting or attaching to local server..."),
            Self::BusyAttempt { attempt_id } => write!(
                f,
                "connection attempt {attempt_id} is still finishing; retry shortly"
            ),
            Self::AttemptIdsExhausted => {
                f.write_str("could not start connection: attempt identifiers exhausted")
            }
            Self::WorkerStartFailed(error) => {
                write!(f, "could not start connection worker: {error}")
            }
            Self::AttemptTimedOut => {
                f.write_str("connection attempt timed out; waiting for cleanup before reconnecting")
            }
            Self::WaitingForCancelledCleanup => {
                f.write_str("waiting for cancelled connection cleanup")
            }
            Self::AwaitingWelcome => f.write_str("awaiting server welcome"),
            Self::ConnectionSetupFailed(error) => {
                write!(f, "connection setup failed: {error}; you can reconnect")
            }
            Self::ConnectFailed(error) => write!(f, "connect failed: {error}; you can retry"),
            Self::DisconnectOrWait => {
                f.write_str("disconnect or wait for the current connection attempt first")
            }
            Self::Disconnected => f.write_str("disconnected"),
            Self::NotConnected => f.write_str("not connected"),
            Self::NoServerResponse => f.write_str("no server response yet"),
            Self::CommandQueued => f.write_str("command queued"),
            Self::CouldNotQueueCommand(problem) => {
                write!(f, "could not queue command: {problem}")
            }
            Self::CouldNotFire(problem) => write!(f, "could not fire: {problem}"),
            Self::CouldNotQueueMovement(problem) => {
                write!(f, "could not queue movement: {problem}")
            }
            Self::CommandParseError(error) => f.write_str(match error {
                ParseError::MissingChat => "say requires a message",
                ParseError::InvalidLane => "thread count must be 1, 2, or auto",
                ParseError::HostUsage => "usage: host local [--threads 1|2|auto]",
                ParseError::ConnectUsage => "usage: connect HOST:PORT",
                ParseError::MoveUsage => "usage: move X Z",
                ParseError::NameUsage => "usage: name NAME",
                ParseError::InvalidCoordinate => "coordinates must be numbers",
                ParseError::NonFiniteCoordinate => "coordinates must be finite",
                ParseError::UnknownCommand => "unknown command; type help for commands",
            }),
            Self::TerminalIntro => {
                f.write_str("Minimium client console. Type `help` for commands.")
            }
            Self::CommandLineHelp => f.write_str(
                "minimium-client [--connect HOST:PORT] [--map PATH | --world-definition PATH] [--profile-dir PATH] [--server-data-dir PATH] [--threads 1|2|auto] [--demo] [--console] [--require-earth]\nDefault: desktop client with a local server. F1 opens the console. --console selects the text client; host local starts/attaches its server.",
            ),
            Self::InputPrompt => f.write_str("> "),
            Self::ClientError(error) => write!(f, "client error: {error}"),
            Self::MapLoadFailed { path, detail } => {
                write!(f, "could not load map {path}: {detail}")
            }
            Self::BundledMapInvalid(detail) => write!(f, "invalid bundled map: {detail}"),
            Self::ConsoleInputQueueFull => f.write_str("console input queue full; dropping line"),
            Self::ConsoleInputReadFailed(error) => {
                write!(f, "console input read failed: {error}")
            }
            Self::ConsoleInputTooLong { max_bytes } => {
                write!(f, "console input line exceeds {max_bytes} bytes; discarded")
            }
            Self::WindowCreationFailed(error) => write!(f, "window creation failed: {error}"),
            Self::GraphicsInitializationFailed(error) => {
                write!(f, "graphics initialization failed: {error}")
            }
            Self::RendererAlreadyInitialized => {
                f.write_str("renderer can only initialize once")
            }
            Self::RenderFailed(error) => write!(f, "render error: {error}"),
            Self::WindowTitle => f.write_str("Minimium 3D client"),
            Self::WindowTitleStatus { fps, health } => write!(
                f,
                "Minimium | {fps:.1} FPS | {health} HP | WASD move, Space jump, left click fire, Esc release"
            ),
            Self::LocalServerGuardCleanupFailed(error) => {
                write!(f, "could not clean up previous local server: {error}")
            }
            Self::LocalServerCheckFailed(error) => {
                write!(f, "could not check previous local server process: {error}")
            }
            Self::FpsUnavailable => {
                f.write_str("FPS display is unavailable in the terminal client")
            }
            Self::ConsoleHistoryHelp => {
                f.write_str("F1 closes the console. Arrow keys browse command history.")
            }
            Self::ShotFired => f.write_str("shot fired"),
            Self::VisiblePlayerLimitExceeded => {
                f.write_str("server exceeded the visible-player snapshot limit")
            }
            Self::WelcomeTimeout => f.write_str("server welcome timed out; you can reconnect"),
            Self::HeartbeatTimeout => f.write_str("server heartbeat timed out; you can reconnect"),
            Self::PeerDisconnected => f.write_str("server disconnected; you can reconnect"),
            Self::ServerClosed => f.write_str("server closed the session; you can reconnect"),
            Self::Rejected { code } => write!(
                f,
                "server rejected connection: {}",
                server_error_text(*code)
            ),
            Self::ServerError { code } => {
                write!(f, "server error: {}", server_error_text(*code))
            }
            Self::MapMismatch { expected, actual } => write!(
                f,
                "map mismatch: client={expected:016x} server={actual:016x}"
            ),
            Self::ConnectionLost(reason) => {
                write!(f, "connection lost ({reason}); you can reconnect")
            }
            Self::Connected { id, health } => {
                write!(f, "connected as player {id} with {health} HP")
            }
            Self::Ping { milliseconds } => write!(f, "ping: {milliseconds} ms"),
            Self::NetworkStats {
                bytes_sent,
                bytes_received,
                ping_milliseconds,
                nearby_players,
            } => {
                write!(f, "tx={bytes_sent} bytes rx={bytes_received} bytes rtt=")?;
                match ping_milliseconds {
                    Some(milliseconds) => write!(f, "{milliseconds} ms")?,
                    None => f.write_str("n/a")?,
                }
                if let Some(count) = nearby_players {
                    write!(f, " nearby_players={count}")?;
                }
                Ok(())
            }
            Self::Pong => f.write_str("PONG"),
            Self::Accepted => f.write_str("server accepted command"),
            Self::Health { health } => write!(f, "health: {health} HP"),
            Self::ShotMissed => f.write_str("shot missed"),
            Self::Hit { target_id, health } => {
                write!(f, "hit player {target_id} ({health} HP)")
            }
            Self::TargetRespawned { target_id, health } => {
                write!(f, "hit player {target_id}; target respawned ({health} HP)")
            }
            Self::Chat {
                sender_name,
                sender_id,
                text,
            } => {
                write!(f, "[{sender_name}#{sender_id}] {text}")
            }
            Self::TerminalSnapshot {
                server_time_ms,
                full_snapshot,
                self_position,
                self_velocity,
                grounded,
                player_updates,
                player_removals,
            } => write!(
                f,
                "server_time={server_time_ms}ms full={full_snapshot} \
                 self=({:.2},{:.2},{:.2}) velocity=({:.2},{:.2},{:.2}) \
                 grounded={grounded} player_updates={player_updates} player_removals={player_removals}",
                self_position[0],
                self_position[1],
                self_position[2],
                self_velocity[0],
                self_velocity[1],
                self_velocity[2]
            ),
            Self::PlayerState {
                position,
                health,
                nearby_players,
            } => write!(
                f,
                "position=({:.2},{:.2},{:.2}) health={health} HP nearby_players={nearby_players}",
                position[0], position[1], position[2]
            ),
        }
    }
}
