//! Shared English display messages for the terminal and rendered clients.
use crate::session::{DisconnectReason, QueueError};
use minimium_localization::UiMessage;
use minimium_protocol::{ProtocolError, ServerErrorCode};

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
    #[cfg(feature = "desktop")]
    CommandQueued,
    CouldNotQueueCommand(QueueProblem),
    #[cfg(feature = "desktop")]
    CouldNotFire(QueueProblem),
    TerminalIntro,
    InputPrompt,
    ClientError(String),
    MapLoadFailed {
        path: String,
        detail: String,
    },
    BundledMapInvalid(String),
    #[cfg(feature = "desktop")]
    WindowCreationFailed(String),
    #[cfg(feature = "desktop")]
    GraphicsInitializationFailed(String),
    #[cfg(feature = "desktop")]
    RendererAlreadyInitialized,
    #[cfg(feature = "desktop")]
    WindowTitle,
    #[cfg(feature = "desktop")]
    WindowTitleStatus {
        fps: f64,
        health: u16,
    },
    #[cfg(feature = "desktop")]
    CouldNotQueueMovement(QueueProblem),
    FpsUnavailable,
    #[cfg(feature = "desktop")]
    ConsoleHistoryHelp,
    #[cfg(feature = "desktop")]
    ShotFired,
    #[cfg(feature = "desktop")]
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
    #[cfg(feature = "desktop")]
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
    AuthenticationReserved,
    StaleWorldContext,
    BufferFull,
    InputSequenceExhausted,
    Protocol(ProtocolError),
}

impl From<QueueError> for QueueProblem {
    fn from(error: QueueError) -> Self {
        match error {
            QueueError::Disconnected => Self::Disconnected,
            QueueError::AwaitingWelcome => Self::AwaitingWelcome,
            QueueError::AuthenticateReserved => Self::AuthenticationReserved,
            QueueError::StaleWorldContext => Self::StaleWorldContext,
            QueueError::BufferFull => Self::BufferFull,
            QueueError::InputSequenceExhausted => Self::InputSequenceExhausted,
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
            DisconnectReason::EnvironmentMismatch => Self::ConnectionLost(
                "server environment model or star catalogue is incompatible".to_owned(),
            ),
            DisconnectReason::PeerClosed => Self::PeerDisconnected,
            DisconnectReason::ServerClosed => Self::ServerClosed,
            DisconnectReason::WorldContent(error)
            | DisconnectReason::EarthPackage(error)
            | DisconnectReason::Materials(error) => Self::ConnectionLost(error.to_string()),
            other => Self::ConnectionLost(format!("{other:?}")),
        }
    }
}

impl ClientText {
    pub fn localized(
        &self,
        localizer: &minimium_localization::Localizer,
        locale: minimium_localization::Locale,
    ) -> String {
        match self {
            Self::ConnectingTo(address) => localizer.format(
                locale,
                &minimium_localization::UiMessage::ConnectingTo(address.clone()),
            ),
            Self::Disconnected => localizer.format(
                locale,
                &minimium_localization::UiMessage::ServerDisconnected,
            ),
            Self::StartingLocalServer => localizer.format(
                locale,
                &minimium_localization::UiMessage::StartingLocalServer,
            ),
            Self::AwaitingWelcome => {
                localizer.format(locale, &minimium_localization::UiMessage::AwaitingWelcome)
            }
            Self::NotConnected => {
                localizer.format(locale, &minimium_localization::UiMessage::NotConnected)
            }
            Self::NoServerResponse => {
                localizer.format(locale, &minimium_localization::UiMessage::NoServerResponse)
            }
            Self::TerminalIntro => {
                localizer.format(locale, &minimium_localization::UiMessage::TerminalIntro)
            }
            Self::InputPrompt => {
                localizer.format(locale, &minimium_localization::UiMessage::InputPrompt)
            }
            #[cfg(feature = "desktop")]
            Self::WindowTitle => {
                localizer.format(locale, &minimium_localization::UiMessage::WindowTitle)
            }
            #[cfg(feature = "desktop")]
            Self::WindowCreationFailed(detail) => localizer.format(
                locale,
                &minimium_localization::UiMessage::WindowCreationFailed(detail.clone()),
            ),
            Self::BusyAttempt { attempt_id } => {
                localizer.format(locale, &UiMessage::BusyAttempt(*attempt_id))
            }
            Self::AttemptIdsExhausted => localizer.format(locale, &UiMessage::AttemptIdsExhausted),
            Self::WorkerStartFailed(value) => {
                localizer.format(locale, &UiMessage::WorkerStartFailed(value.clone()))
            }
            Self::AttemptTimedOut => localizer.format(locale, &UiMessage::AttemptTimedOut),
            Self::WaitingForCancelledCleanup => {
                localizer.format(locale, &UiMessage::WaitingForCleanup)
            }
            Self::ConnectionSetupFailed(value) => {
                localizer.format(locale, &UiMessage::ConnectionSetupFailed(value.clone()))
            }
            Self::ConnectFailed(value) => {
                localizer.format(locale, &UiMessage::ConnectFailed(value.clone()))
            }
            Self::DisconnectOrWait => localizer.format(locale, &UiMessage::DisconnectOrWait),
            #[cfg(feature = "desktop")]
            Self::CommandQueued => localizer.format(locale, &UiMessage::CommandQueued),
            Self::CouldNotQueueCommand(value) => localize_queue(
                localizer,
                locale,
                minimium_localization::QueueAction::Command,
                value,
            ),
            #[cfg(feature = "desktop")]
            Self::CouldNotFire(value) => localize_queue(
                localizer,
                locale,
                minimium_localization::QueueAction::Fire,
                value,
            ),
            #[cfg(feature = "desktop")]
            Self::CouldNotQueueMovement(value) => localize_queue(
                localizer,
                locale,
                minimium_localization::QueueAction::Movement,
                value,
            ),
            Self::ClientError(value) => {
                localizer.format(locale, &UiMessage::ClientError(value.clone()))
            }
            Self::MapLoadFailed { path, detail } => localizer.format(
                locale,
                &UiMessage::MapLoadFailed {
                    path: path.clone(),
                    detail: detail.clone(),
                },
            ),
            Self::BundledMapInvalid(value) => {
                localizer.format(locale, &UiMessage::BundledMapInvalid(value.clone()))
            }
            #[cfg(feature = "desktop")]
            Self::GraphicsInitializationFailed(value) => localizer.format(
                locale,
                &UiMessage::GraphicsInitializationFailed(value.clone()),
            ),
            #[cfg(feature = "desktop")]
            Self::RendererAlreadyInitialized => {
                localizer.format(locale, &UiMessage::RendererAlreadyInitialized)
            }
            #[cfg(feature = "desktop")]
            Self::WindowTitleStatus { fps, health } => localizer.format(
                locale,
                &UiMessage::WindowTitleStatus {
                    fps: format!("{fps:.1}"),
                    health: *health,
                },
            ),
            Self::FpsUnavailable => localizer.format(locale, &UiMessage::FpsUnavailable),
            #[cfg(feature = "desktop")]
            Self::ConsoleHistoryHelp => localizer.format(locale, &UiMessage::ConsoleHistoryHelp),
            #[cfg(feature = "desktop")]
            Self::ShotFired => localizer.format(locale, &UiMessage::ShotFired),
            #[cfg(feature = "desktop")]
            Self::VisiblePlayerLimitExceeded => {
                localizer.format(locale, &UiMessage::VisiblePlayerLimitExceeded)
            }
            Self::TerminalSnapshot {
                server_time_ms,
                full_snapshot,
                self_position,
                self_velocity,
                grounded,
                player_updates,
                player_removals,
            } => localizer.format(
                locale,
                &UiMessage::TerminalSnapshot {
                    time: *server_time_ms,
                    full: *full_snapshot,
                    position: format!(
                        "{:.2},{:.2},{:.2}",
                        self_position[0], self_position[1], self_position[2]
                    ),
                    velocity: format!(
                        "{:.2},{:.2},{:.2}",
                        self_velocity[0], self_velocity[1], self_velocity[2]
                    ),
                    grounded: *grounded,
                    updates: *player_updates,
                    removals: *player_removals,
                },
            ),
            #[cfg(feature = "desktop")]
            Self::PlayerState {
                position,
                health,
                nearby_players,
            } => localizer.format(
                locale,
                &UiMessage::PlayerState {
                    position: format!("{:.2},{:.2},{:.2}", position[0], position[1], position[2]),
                    health: *health,
                    nearby: *nearby_players,
                },
            ),
            Self::WelcomeTimeout => localizer.format(locale, &UiMessage::WelcomeTimeout),
            Self::HeartbeatTimeout => localizer.format(locale, &UiMessage::HeartbeatTimeout),
            Self::PeerDisconnected => localizer.format(locale, &UiMessage::PeerDisconnected),
            Self::ServerClosed => localizer.format(locale, &UiMessage::ServerClosed),
            Self::Rejected { code } => {
                localizer.format(locale, &UiMessage::Rejected(localized_server_error(*code)))
            }
            Self::ServerError { code } => localizer.format(
                locale,
                &UiMessage::ServerError(localized_server_error(*code)),
            ),
            Self::MapMismatch { expected, actual } => localizer.format(
                locale,
                &UiMessage::MapMismatch {
                    expected: format!("{expected:016x}"),
                    actual: format!("{actual:016x}"),
                },
            ),
            Self::ConnectionLost(value) => {
                localizer.format(locale, &UiMessage::ConnectionLost(value.clone()))
            }
            Self::Connected { id, health } => localizer.format(
                locale,
                &UiMessage::Connected {
                    id: id.to_string(),
                    health: *health,
                },
            ),
            Self::Ping { milliseconds } => {
                localizer.format(locale, &UiMessage::Ping(*milliseconds))
            }
            Self::NetworkStats {
                bytes_sent,
                bytes_received,
                ping_milliseconds,
                nearby_players,
            } => localizer.format(
                locale,
                &UiMessage::NetworkStats {
                    sent: *bytes_sent,
                    received: *bytes_received,
                    ping: ping_milliseconds
                        .map_or_else(|| "n/a".to_owned(), |value| format!("{value} ms")),
                    nearby: nearby_players.map_or_else(String::new, |value| value.to_string()),
                    nearby_active: nearby_players.is_some(),
                },
            ),
            Self::Pong => localizer.format(locale, &UiMessage::Pong),
            Self::Accepted => localizer.format(locale, &UiMessage::Accepted),
            Self::Health { health } => localizer.format(locale, &UiMessage::Health(*health)),
            Self::ShotMissed => localizer.format(locale, &UiMessage::ShotMissed),
            Self::Hit { target_id, health } => localizer.format(
                locale,
                &UiMessage::Hit {
                    target: target_id.to_string(),
                    health: *health,
                },
            ),
            Self::TargetRespawned { target_id, health } => localizer.format(
                locale,
                &UiMessage::TargetRespawned {
                    target: target_id.to_string(),
                    health: *health,
                },
            ),
            Self::Chat {
                sender_name,
                sender_id,
                text,
            } => localizer.format(
                locale,
                &UiMessage::Chat {
                    name: sender_name.clone(),
                    id: sender_id.to_string(),
                    text: text.clone(),
                },
            ),
        }
    }
}

fn localized_server_error(code: ServerErrorCode) -> minimium_localization::ServerErrorReason {
    use minimium_localization::ServerErrorReason as Reason;
    match code {
        ServerErrorCode::Protocol => Reason::Protocol,
        ServerErrorCode::Busy => Reason::Busy,
        ServerErrorCode::ServerFull => Reason::Full,
        ServerErrorCode::RateLimit => Reason::RateLimit,
        ServerErrorCode::Unknown(_) => Reason::Other,
    }
}

fn localize_queue(
    localizer: &minimium_localization::Localizer,
    locale: minimium_localization::Locale,
    action: minimium_localization::QueueAction,
    problem: &QueueProblem,
) -> String {
    use minimium_localization::QueueCause as Cause;
    let cause = match problem {
        QueueProblem::Disconnected => Cause::Disconnected,
        QueueProblem::AwaitingWelcome => Cause::AwaitingWelcome,
        QueueProblem::AuthenticationReserved => Cause::Authentication,
        QueueProblem::StaleWorldContext => Cause::StaleWorld,
        QueueProblem::BufferFull => Cause::Full,
        QueueProblem::InputSequenceExhausted => Cause::Sequence,
        QueueProblem::Protocol(_) => Cause::Protocol,
    };
    localizer.format(locale, &UiMessage::QueueFailure { action, cause })
}
