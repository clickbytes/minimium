use crate::{QueueAction, QueueCause, ServerErrorReason, UiNotice, UiStatus};
use fluent_bundle::{FluentArgs, FluentValue};

#[derive(Clone, Debug, PartialEq)]
pub enum UiMessage {
    ConnectingTo(String),
    ItemsAvailable(u64),
    ServerDisconnected,
    StartingLocalServer,
    AwaitingWelcome,
    NotConnected,
    NoServerResponse,
    TerminalIntro,
    InputPrompt,
    WindowTitle,
    WindowCreationFailed(String),
    BusyAttempt(u64),
    AttemptIdsExhausted,
    WorkerStartFailed(String),
    AttemptTimedOut,
    WaitingForCleanup,
    ConnectionSetupFailed(String),
    ConnectFailed(String),
    DisconnectOrWait,
    CommandQueued,
    ClientError(String),
    MapLoadFailed {
        path: String,
        detail: String,
    },
    BundledMapInvalid(String),
    GraphicsInitializationFailed(String),
    RendererAlreadyInitialized,
    RenderFailed(String),
    FpsUnavailable,
    ConsoleHistoryHelp,
    ShotFired,
    VisiblePlayerLimitExceeded,
    WelcomeTimeout,
    HeartbeatTimeout,
    PeerDisconnected,
    ServerClosed,
    Rejected(ServerErrorReason),
    ServerError(ServerErrorReason),
    MapMismatch {
        expected: String,
        actual: String,
    },
    ConnectionLost(String),
    Connected {
        id: String,
        health: u16,
    },
    Ping(u128),
    Pong,
    Accepted,
    Health(u16),
    ShotMissed,
    Hit {
        target: String,
        health: u16,
    },
    TargetRespawned {
        target: String,
        health: u16,
    },
    QueueFailure {
        action: QueueAction,
        cause: QueueCause,
    },
    WindowTitleStatus {
        fps: String,
        health: u16,
    },
    TerminalSnapshot {
        time: u64,
        full: bool,
        position: String,
        velocity: String,
        grounded: bool,
        updates: usize,
        removals: usize,
    },
    PlayerState {
        position: String,
        health: u16,
        nearby: usize,
    },
    NetworkStats {
        sent: u64,
        received: u64,
        ping: String,
        nearby: String,
        nearby_active: bool,
    },
    Chat {
        name: String,
        id: String,
        text: String,
    },
    AltitudeResult(bool),
    WorldTransferPrepared,
    WorldTransferCommitted,
    WorldTransferCancelled,
    Status(UiStatus),
    EarthError(String),
    Notice(UiNotice),
    PropertyDescription(String),
    NoPropertyMatches(String),
    CommandHelp(String),
    CommandRejected {
        code: String,
        subject: String,
    },
    MapTeleportResult(bool),
    ContentUnavailable,
    ConfigurationSaved(String),
    ConfigurationSaveFailed,
    EnvironmentStatus {
        quality: String,
        date: String,
        rate: String,
        revision: String,
    },
    EnvironmentTitle {
        quality: String,
        date: String,
    },
}

impl UiMessage {
    pub(super) fn key(&self) -> &'static str {
        match self {
            Self::ConnectingTo(_) => "connection-connecting",
            Self::ItemsAvailable(_) => "items-available",
            Self::ServerDisconnected => "connection-disconnected",
            Self::StartingLocalServer => "connection-starting-local",
            Self::AwaitingWelcome => "connection-awaiting-welcome",
            Self::NotConnected => "connection-not-connected",
            Self::NoServerResponse => "connection-no-response",
            Self::TerminalIntro => "terminal-intro",
            Self::InputPrompt => "terminal-prompt",
            Self::WindowTitle => "window-title",
            Self::WindowCreationFailed(_) => "window-create-failed",
            Self::BusyAttempt(_) => "connection-attempt-busy",
            Self::AttemptIdsExhausted => "connection-attempts-exhausted",
            Self::WorkerStartFailed(_) => "connection-worker-start-failed",
            Self::AttemptTimedOut => "connection-timeout",
            Self::WaitingForCleanup => "connection-waiting-cleanup",
            Self::ConnectionSetupFailed(_) => "connection-setup-failed",
            Self::ConnectFailed(_) => "connection-failed",
            Self::DisconnectOrWait => "connection-disconnect-or-wait",
            Self::CommandQueued => "command-queued",
            Self::ClientError(_) => "client-error",
            Self::MapLoadFailed { .. } => "map-load-failed",
            Self::BundledMapInvalid(_) => "bundled-map-invalid",
            Self::GraphicsInitializationFailed(_) => "graphics-init-failed",
            Self::RendererAlreadyInitialized => "renderer-already-initialized",
            Self::RenderFailed(_) => "render-failed",
            Self::FpsUnavailable => "fps-unavailable",
            Self::ConsoleHistoryHelp => "console-history-help",
            Self::ShotFired => "shot-fired",
            Self::VisiblePlayerLimitExceeded => "visible-player-limit-exceeded",
            Self::WelcomeTimeout => "welcome-timeout",
            Self::HeartbeatTimeout => "heartbeat-timeout",
            Self::PeerDisconnected => "peer-disconnected",
            Self::ServerClosed => "server-closed",
            Self::Rejected(_) => "connection-rejected",
            Self::ServerError(_) => "server-error",
            Self::MapMismatch { .. } => "map-mismatch",
            Self::ConnectionLost(_) => "connection-lost",
            Self::Connected { .. } => "connection-connected",
            Self::Ping(_) => "network-ping",
            Self::Pong => "network-pong",
            Self::Accepted => "server-accepted",
            Self::Health(_) => "player-health",
            Self::ShotMissed => "shot-missed",
            Self::Hit { .. } => "shot-hit",
            Self::TargetRespawned { .. } => "shot-target-respawned",
            Self::QueueFailure { .. } => "queue-failure",
            Self::WindowTitleStatus { .. } => "window-title-status",
            Self::TerminalSnapshot { .. } => "terminal-snapshot",
            Self::PlayerState { .. } => "player-state",
            Self::NetworkStats { .. } => "network-stats",
            Self::Chat { .. } => "chat-line",
            Self::AltitudeResult(_) => "altitude-result",
            Self::WorldTransferPrepared => "world-transfer-prepared",
            Self::WorldTransferCommitted => "world-transfer-committed",
            Self::WorldTransferCancelled => "world-transfer-cancelled",
            Self::Status(_) => "ui-status",
            Self::EarthError(_) => "earth-error",
            Self::Notice(_) => "ui-notice",
            Self::PropertyDescription(_) => "property-description",
            Self::NoPropertyMatches(_) => "no-property-matches",
            Self::CommandHelp(_) => "command-help",
            Self::CommandRejected { .. } => "command-rejected",
            Self::MapTeleportResult(_) => "map-teleport-result",
            Self::ContentUnavailable => "content-unavailable",
            Self::ConfigurationSaved(_) => "configuration-saved",
            Self::ConfigurationSaveFailed => "configuration-save-failed",
            Self::EnvironmentStatus { .. } => "environment-status",
            Self::EnvironmentTitle { .. } => "environment-title",
        }
    }
    pub(super) fn args(&self) -> FluentArgs<'static> {
        let mut args = FluentArgs::new();
        match self {
            Self::ConnectingTo(address) => args.set("address", FluentValue::from(address.clone())),
            Self::ItemsAvailable(count) => args.set("count", FluentValue::from(*count as i64)),
            Self::ServerDisconnected => {}
            Self::StartingLocalServer
            | Self::AwaitingWelcome
            | Self::NotConnected
            | Self::NoServerResponse
            | Self::TerminalIntro
            | Self::InputPrompt => {}
            Self::WindowTitle => {}
            Self::WindowCreationFailed(detail) => {
                args.set("detail", FluentValue::from(detail.clone()))
            }
            Self::BusyAttempt(id) => args.set("id", FluentValue::from(*id as i64)),
            Self::WorkerStartFailed(detail)
            | Self::ConnectionSetupFailed(detail)
            | Self::ConnectFailed(detail)
            | Self::ClientError(detail)
            | Self::BundledMapInvalid(detail)
            | Self::GraphicsInitializationFailed(detail)
            | Self::RenderFailed(detail)
            | Self::ConnectionLost(detail) => args.set("detail", FluentValue::from(detail.clone())),
            Self::Rejected(reason) | Self::ServerError(reason) => args.set(
                "reason",
                FluentValue::from(match reason {
                    ServerErrorReason::Protocol => "protocol",
                    ServerErrorReason::Busy => "busy",
                    ServerErrorReason::Full => "full",
                    ServerErrorReason::RateLimit => "rate_limit",
                    ServerErrorReason::Other => "other",
                }),
            ),
            Self::MapLoadFailed { path, detail } => {
                args.set("path", FluentValue::from(path.clone()));
                args.set("detail", FluentValue::from(detail.clone()));
            }
            Self::MapMismatch { expected, actual } => {
                args.set("expected", FluentValue::from(expected.clone()));
                args.set("actual", FluentValue::from(actual.clone()));
            }
            Self::Connected { id, health } => {
                args.set("id", FluentValue::from(id.clone()));
                args.set("health", FluentValue::from(*health as i64));
            }
            Self::Ping(milliseconds) => {
                args.set("milliseconds", FluentValue::from(*milliseconds as i64))
            }
            Self::Health(health) => args.set("health", FluentValue::from(*health as i64)),
            Self::Hit { target, health } | Self::TargetRespawned { target, health } => {
                args.set("target", FluentValue::from(target.clone()));
                args.set("health", FluentValue::from(*health as i64));
            }
            Self::QueueFailure { action, cause } => {
                args.set(
                    "action",
                    FluentValue::from(match action {
                        QueueAction::Command => "command",
                        QueueAction::Fire => "fire",
                        QueueAction::Movement => "movement",
                    }),
                );
                args.set(
                    "cause",
                    FluentValue::from(match cause {
                        QueueCause::Disconnected => "disconnected",
                        QueueCause::AwaitingWelcome => "awaiting",
                        QueueCause::Authentication => "authentication",
                        QueueCause::StaleWorld => "stale",
                        QueueCause::Full => "full",
                        QueueCause::Sequence => "sequence",
                        QueueCause::Protocol => "protocol",
                    }),
                );
            }
            Self::WindowTitleStatus { fps, health } => {
                args.set("fps", FluentValue::from(fps.clone()));
                args.set("health", FluentValue::from(*health as i64));
            }
            Self::TerminalSnapshot {
                time,
                full,
                position,
                velocity,
                grounded,
                updates,
                removals,
            } => {
                args.set("time", FluentValue::from(*time as i64));
                args.set("full", FluentValue::from(full.to_string()));
                args.set("position", FluentValue::from(position.clone()));
                args.set("velocity", FluentValue::from(velocity.clone()));
                args.set("grounded", FluentValue::from(grounded.to_string()));
                args.set("updates", FluentValue::from(*updates as i64));
                args.set("removals", FluentValue::from(*removals as i64));
            }
            Self::PlayerState {
                position,
                health,
                nearby,
            } => {
                args.set("position", FluentValue::from(position.clone()));
                args.set("health", FluentValue::from(*health as i64));
                args.set("nearby", FluentValue::from(*nearby as i64));
            }
            Self::NetworkStats {
                sent,
                received,
                ping,
                nearby,
                nearby_active,
            } => {
                args.set("sent", FluentValue::from(*sent as i64));
                args.set("received", FluentValue::from(*received as i64));
                args.set("ping", FluentValue::from(ping.clone()));
                args.set("nearby", FluentValue::from(nearby.clone()));
                args.set(
                    "nearby_active",
                    FluentValue::from(if *nearby_active { "yes" } else { "no" }),
                );
            }
            Self::Chat { name, id, text } => {
                args.set("name", FluentValue::from(name.clone()));
                args.set("id", FluentValue::from(id.clone()));
                args.set("text", FluentValue::from(text.clone()));
            }
            Self::AltitudeResult(success) => args.set(
                "result",
                FluentValue::from(if *success { "success" } else { "rejected" }),
            ),
            Self::WorldTransferPrepared
            | Self::WorldTransferCommitted
            | Self::WorldTransferCancelled => {}
            Self::Status(status) => args.set(
                "status",
                FluentValue::from(match status {
                    UiStatus::EarthConnecting => "earth_connecting",
                    UiStatus::EarthDisconnected => "earth_disconnected",
                    UiStatus::EarthConnected => "earth_connected",
                    UiStatus::Connected => "connected",
                    UiStatus::EarthLoading => "earth_loading",
                    UiStatus::EarthChooseLand => "earth_choose_land",
                    UiStatus::EarthClickLand => "earth_click_land",
                    UiStatus::EarthClickSurface => "earth_click_surface",
                    UiStatus::EarthMoving => "earth_moving",
                    UiStatus::EarthMoved => "earth_moved",
                    UiStatus::TerrainMaterials => "terrain_materials",
                    UiStatus::ConnectionTimedOut => "connection_timed_out",
                    UiStatus::EarthServerClosed => "earth_server_closed",
                    UiStatus::ContentUnavailable => "content_unavailable",
                    UiStatus::EnvironmentInitializing => "environment_initializing",
                    UiStatus::EnvironmentSynchronized => "environment_synchronized",
                    UiStatus::EnvironmentStale => "environment_stale",
                    UiStatus::EnvironmentUnavailable => "environment_unavailable",
                    UiStatus::EnvironmentLocalDemo => "environment_local_demo",
                }),
            ),
            Self::EarthError(detail) => args.set("detail", FluentValue::from(detail.clone())),
            Self::Notice(notice) => args.set(
                "notice",
                FluentValue::from(match notice {
                    UiNotice::ConsoleWelcome => "console_welcome",
                    UiNotice::ConsoleOverview => "console_overview",
                    UiNotice::CommandsHeader => "commands_header",
                    UiNotice::TimeRequestSent => "time_request_sent",
                    UiNotice::TimeScaleRequestSent => "time_scale_request_sent",
                    UiNotice::BolidRequestSent => "bolid_request_sent",
                    UiNotice::BolidRequestQueued => "bolid_request_queued",
                    UiNotice::TimeModeRequestSent => "time_mode_request_sent",
                    UiNotice::TimeOfflinePolicyRequestSent => "time_offline_policy_request_sent",
                    UiNotice::SkyBodiesUnavailable => "sky_bodies_unavailable",
                    UiNotice::MapTeleportDesktopOnly => "map_teleport_desktop_only",
                    UiNotice::SkyInfoDesktopOnly => "sky_info_desktop_only",
                    UiNotice::WeatherRequiresDesktop => "weather_requires_desktop",
                    UiNotice::SkyQualityDesktopOnly => "sky_quality_desktop_only",
                    UiNotice::MapTeleportBusy => "map_teleport_busy",
                    UiNotice::MapTeleportRequiresEarth => "map_teleport_requires_earth",
                    UiNotice::LocalServerThreadsConflict => "local_server_threads_conflict",
                    UiNotice::StarCatalogUnavailable => "star_catalog_unavailable",
                    UiNotice::TimeRequestFailed => "time_request_failed",
                    UiNotice::TimeScaleRequestFailed => "time_scale_request_failed",
                    UiNotice::TimeModeRequestFailed => "time_mode_request_failed",
                    UiNotice::TimeOfflinePolicyRequestFailed => {
                        "time_offline_policy_request_failed"
                    }
                    UiNotice::TimeScaleResponseFailed => "time_scale_response_failed",
                    UiNotice::EnvironmentUnavailable => "environment_unavailable",
                    UiNotice::ClipboardUnavailable => "clipboard_unavailable",
                    UiNotice::AltitudeRequiresFlight => "altitude_requires_flight",
                    UiNotice::PortalSnapshotUnavailable => "portal_snapshot_unavailable",
                    UiNotice::PortalUnavailable => "portal_unavailable",
                    UiNotice::LocalServerConfigFailed => "local_server_config_failed",
                }),
            ),
            Self::PropertyDescription(key) => {
                args.set("key", FluentValue::from(key.replace('.', "_")))
            }
            Self::NoPropertyMatches(prefix) => {
                args.set("prefix", FluentValue::from(prefix.clone()))
            }
            Self::CommandHelp(name) => args.set(
                "name",
                FluentValue::from(name.replace(' ', "_").replace('.', "_")),
            ),
            Self::CommandRejected { code, subject } => {
                args.set("code", FluentValue::from(code.clone()));
                args.set("subject", FluentValue::from(subject.clone()));
            }
            Self::MapTeleportResult(success) => args.set(
                "result",
                FluentValue::from(if *success { "success" } else { "rejected" }),
            ),
            Self::ContentUnavailable => {}
            Self::ConfigurationSaved(path) => args.set("path", FluentValue::from(path.clone())),
            Self::ConfigurationSaveFailed => {}
            Self::EnvironmentStatus {
                quality,
                date,
                rate,
                revision,
            } => {
                args.set("quality", FluentValue::from(quality.clone()));
                args.set("date", FluentValue::from(date.clone()));
                args.set("rate", FluentValue::from(rate.clone()));
                args.set("revision", FluentValue::from(revision.clone()));
            }
            Self::EnvironmentTitle { quality, date } => {
                args.set("quality", FluentValue::from(quality.clone()));
                args.set("date", FluentValue::from(date.clone()));
            }
            Self::AttemptIdsExhausted
            | Self::AttemptTimedOut
            | Self::WaitingForCleanup
            | Self::DisconnectOrWait
            | Self::CommandQueued
            | Self::RendererAlreadyInitialized
            | Self::FpsUnavailable
            | Self::ConsoleHistoryHelp
            | Self::ShotFired
            | Self::VisiblePlayerLimitExceeded
            | Self::WelcomeTimeout
            | Self::HeartbeatTimeout
            | Self::PeerDisconnected
            | Self::ServerClosed
            | Self::Pong
            | Self::Accepted
            | Self::ShotMissed => {}
        }
        args
    }
}
