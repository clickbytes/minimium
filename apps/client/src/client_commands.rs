//! Shared typed command parsing and dispatch for terminal and graphical clients.
//!
//! [`parse_command`] accepts the union of the terminal and F1 console commands.
//! A startup registry is the source for help, aliases, and completion. Execution
//! sends world-action intents through the application executor and reports UI/process work as a typed
//! [`FrontendAction`], leaving each front end in control of its own event loop.

use crate::application::SuppressionReason;
use crate::application::{ClientAction, LocalActionOutcome, QueuedActionKind};
use crate::{options::ThreadLane, session::QueueError};
use minimium_commands::{
    lex_command_line_with_spans, CommandError, CommandErrorCode, CommandOutcome,
    CommandRegistry as SharedCommandRegistry, CommandSpec as SharedCommandSpec,
};
use minimium_renderer::quality::SkyQuality;
use std::sync::OnceLock;

mod bindings;
pub(crate) mod camera;
mod catalog;
mod explain;
#[cfg(feature = "desktop")]
pub(crate) use explain::{analyze as analyze_console, validate_edit};
mod connection;
pub(crate) mod connection_adapter;
mod environment;
mod gameplay;
mod help;
mod interface;
mod meteors;
mod movement;
pub(crate) mod output;
pub(crate) mod player_actions;
mod presentation;
mod properties;
mod text;
pub(crate) mod time_adapter;
use minimium_environment::clock_command::ClockWrite;
use time_adapter::ClockAction;
mod weather;

pub(crate) use gameplay::nearest_portal_entity;
pub(crate) use help::command_error;
pub(crate) use meteors::{MeteorDebug, MeteorLightDebug, MeteorPreset};
pub(crate) use minimium_commands::{
    CommandArgumentSpec as ArgumentSpec, CommandArgumentType as ArgumentType,
};
#[cfg(feature = "desktop")]
pub(crate) use presentation::PresentationRequirement;
#[cfg(feature = "desktop")]
pub(crate) use presentation::{BolidPath, DeveloperOperation};
pub(crate) use presentation::{
    PresentationCapability, PresentationOperation, PresentationResult, PresentationUnavailable,
    PresentationUnavailableReason,
};
pub(crate) use properties::local_setting_specs;
pub(crate) use properties::{
    format_property_help_localized, format_time_value, parse_setting_value, property_spec,
    ClientPropertyCommand, ClientPropertyKey, ClientPropertyValue,
};
pub(crate) use text::{
    execute_text, CommandActionResult, CommandOutput, CommandSource, TextCommandResult,
};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ClientCommand {
    Presentation(PresentationOperation),
    AtmosphereDebug(minimium_renderer::atmosphere_debug::AtmosphereDebug),
    TerrainDebug(minimium_renderer::terrain_debug::TerrainDebug),
    Property(properties::ClientPropertyCommand),
    SetAltitude(f32),
    ConfigSave(std::path::PathBuf),
    Help,
    Print(String),
    HostLocal(Option<ThreadLane>),
    Connect(String),
    Disconnect,
    FpsShow,
    NetStats,
    Ping,
    Move([f32; 2]),
    Jump,
    InteractPortal,
    MapTeleport,
    Fire,
    GrantTools,
    Chat(String),
    SetName(String),
    State,
    Clear,
    Quit,
    SkyQuality(SkyQuality),
    SkyDemo(bool),
    Fov(Option<f32>),
    ShowCelestialNames(bool),
    CameraLook { yaw: f32, pitch: f32 },
    CameraStatus,
    InitialClockConfig,
    SetInstant(minimium_environment::time::WorldTimeMs),
    TimeStatus,
    TimeScale(String),
    SpawnRandomBolid(u16),
    SpawnRandomMeteorid,
    SetTime { minute: u16, local: bool },
    WeatherDebug(minimium_renderer::cloud_presentation::CloudDebug),
    WeatherDetail(u8),
}

pub(crate) type CommandSpec = SharedCommandSpec<ClientCommand, ParseError>;
pub(super) type CommandRegistry = SharedCommandRegistry<ClientCommand, ParseError>;

static REGISTRY: OnceLock<CommandRegistry> = OnceLock::new();

pub(crate) fn parse_fov_degrees(value: &str) -> Result<f32, &'static str> {
    let degrees = value.parse::<f32>().map_err(|_| "expected a number")?;
    camera::validate_fov(degrees)?;
    Ok(degrees)
}

pub(crate) fn initialize_registry() {
    let _ = catalog::catalog();
}

fn registry() -> &'static CommandRegistry {
    REGISTRY.get_or_init(|| {
        let mut registry = CommandRegistry::default();
        interface::register(&mut registry);
        connection::register(&mut registry);
        gameplay::register(&mut registry);
        movement::register(&mut registry);
        environment::register(&mut registry);
        weather::register(&mut registry);
        registry
    })
}

pub(crate) fn command_specs() -> &'static [CommandSpec] {
    registry().commands()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ParseError {
    MissingChat,
    InvalidLane,
    HostUsage,
    ConnectUsage,
    MoveUsage,
    NameUsage,
    InvalidCoordinate,
    NonFiniteCoordinate,
    SkyQualityUsage,
    FovUsage,
    CelestialNamesUsage,
    TimeScaleUsage,
    SetTimeUsage,
    MeteorUsage,
    UnknownCommand,
    Usage(&'static str),
    Structured(CommandError),
}

pub(crate) fn parse_command(input: &str) -> Result<Option<ClientCommand>, ParseError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let tokens = lex_command_line_with_spans(trimmed).map_err(ParseError::Structured)?;
    let words = tokens
        .iter()
        .map(|token| token.value.as_str())
        .collect::<Vec<_>>();
    if matches!(
        words.first().copied(),
        Some("get" | "set" | "reset" | "list")
    ) || matches!(words.first().copied(), Some("help"))
    {
        if let Some(command) = properties::parse(trimmed)? {
            return Ok(Some(command));
        }
    }
    let group: Vec<_> = command_specs()
        .iter()
        .filter(|spec| spec.name.starts_with(&format!("{trimmed} ")))
        .map(SharedCommandSpec::format_help)
        .collect();
    if !group.is_empty() {
        return Ok(Some(ClientCommand::Print(group.join("\n"))));
    }
    match registry()
        .parse_line(trimmed)
        .map_err(ParseError::Structured)?
    {
        CommandOutcome::Matched { usage, result, .. } => result
            .map_err(|error| match error {
                ParseError::Structured(error) => ParseError::Structured(error),
                _ => ParseError::Usage(usage),
            })
            .map(Some),
        CommandOutcome::Unknown => {
            let domain = words[0];
            let choices: Vec<_> = command_specs()
                .iter()
                .filter(|spec| spec.name.split_whitespace().next() == Some(domain))
                .map(SharedCommandSpec::format_help)
                .collect();
            let choices = if choices.is_empty() {
                command_specs()
                    .iter()
                    .map(SharedCommandSpec::format_help)
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                choices.join("\n")
            };
            Err(ParseError::Structured(CommandError::new(
                CommandErrorCode::UnknownCommand,
                Some(domain),
                format!("unknown command; available commands:\n{choices}"),
                Some("help"),
            )))
        }
    }
}

pub(crate) struct CommandContext<'a> {
    pub movement: minimium_simulation::MovementPolicy,
    pub flight_enabled: bool,
    pub portal_entity: Option<minimium_world::entity::EntityId>,
    pub execute_action: &'a mut dyn FnMut(ClientAction) -> LocalActionOutcome,
    /// Direction used for `fire`; terminal callers pass [1, 0, 0].
    pub fire_direction: [f32; 3],
    pub player_position: Option<[f32; 3]>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum FrontendAction {
    Notice(minimium_localization::UiNotice),
    Help,
    Print(String),
    Connect(String),
    HostLocal(Option<ThreadLane>),
    Disconnect,
    FpsShow,
    Player(player_actions::PlayerIntent),
    Ping,
    NetStats,
    State,
    MapTeleport,
    Clear,
    Quit,
    Property(properties::ClientPropertyCommand),
    ConfigSave(std::path::PathBuf),
    Clock(ClockAction),
    Jump,
    Movement(
        minimium_client_runtime::MovementField,
        Option<minimium_protocol::MovementPatch>,
    ),
    CameraLook {
        yaw: f32,
        pitch: f32,
    },
    CameraStatus,
    SpawnRandomBolid(u16),
    SpawnRandomMeteorid,

    Presentation(PresentationOperation),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CommandResult {
    Queued {
        kind: QueuedActionKind,
    },
    QueueRejected {
        kind: QueuedActionKind,
        error: QueueError,
    },
    Suppressed(SuppressionReason),
    Action(FrontendAction),
}

pub(crate) fn execute(command: ClientCommand, context: &mut CommandContext<'_>) -> CommandResult {
    let action = match command {
        ClientCommand::Presentation(operation) => {
            return CommandResult::Action(FrontendAction::Presentation(operation));
        }
        ClientCommand::AtmosphereDebug(mode) => {
            return CommandResult::Action(FrontendAction::Property(ClientPropertyCommand::Set(
                ClientPropertyValue::AtmosphereDebug(mode),
            )))
        }
        ClientCommand::SetAltitude(km) => {
            if !context.flight_enabled || !context.movement.can_fly {
                return CommandResult::Action(FrontendAction::Notice(
                    minimium_localization::UiNotice::AltitudeRequiresFlight,
                ));
            }
            ClientAction::SetAltitude { km }
        }
        ClientCommand::ConfigSave(path) => {
            return CommandResult::Action(FrontendAction::ConfigSave(path))
        }
        ClientCommand::Print(text) => return CommandResult::Action(FrontendAction::Print(text)),
        ClientCommand::TerrainDebug(mode) => {
            return CommandResult::Action(FrontendAction::Property(ClientPropertyCommand::Set(
                ClientPropertyValue::TerrainDebug(mode),
            )))
        }
        ClientCommand::WeatherDebug(mode) => {
            return CommandResult::Action(FrontendAction::Property(ClientPropertyCommand::Set(
                ClientPropertyValue::CloudDebug(mode),
            )))
        }
        ClientCommand::WeatherDetail(level) => {
            return CommandResult::Action(FrontendAction::Property(ClientPropertyCommand::Set(
                ClientPropertyValue::CloudDetail(level),
            )))
        }
        ClientCommand::Ping => return CommandResult::Action(FrontendAction::Ping),
        ClientCommand::Move(direction) => {
            return CommandResult::Action(FrontendAction::Player(
                player_actions::PlayerIntent::Move(direction),
            ))
        }
        ClientCommand::Jump => return CommandResult::Action(FrontendAction::Jump),
        ClientCommand::InteractPortal => {
            if context.player_position.is_none() {
                return CommandResult::Action(FrontendAction::Notice(
                    minimium_localization::UiNotice::PortalSnapshotUnavailable,
                ));
            }
            let Some(entity) = context.portal_entity else {
                return CommandResult::Action(FrontendAction::Notice(
                    minimium_localization::UiNotice::PortalUnavailable,
                ));
            };
            ClientAction::InteractEntity { entity }
        }
        ClientCommand::MapTeleport => return CommandResult::Action(FrontendAction::MapTeleport),
        ClientCommand::Fire => {
            return CommandResult::Action(FrontendAction::Player(
                player_actions::PlayerIntent::Fire(context.fire_direction),
            ))
        }
        ClientCommand::GrantTools => {
            return CommandResult::Action(FrontendAction::Player(
                player_actions::PlayerIntent::GrantTools,
            ))
        }
        ClientCommand::Chat(text) => {
            return CommandResult::Action(FrontendAction::Player(
                player_actions::PlayerIntent::Chat(text),
            ))
        }
        ClientCommand::SetName(name) => {
            return CommandResult::Action(FrontendAction::Player(
                player_actions::PlayerIntent::Name(name),
            ))
        }
        ClientCommand::Help => return CommandResult::Action(FrontendAction::Help),
        ClientCommand::Connect(address) => {
            return CommandResult::Action(FrontendAction::Connect(address))
        }
        ClientCommand::HostLocal(lane) => {
            return CommandResult::Action(FrontendAction::HostLocal(lane))
        }
        ClientCommand::Disconnect => return CommandResult::Action(FrontendAction::Disconnect),
        ClientCommand::FpsShow => return CommandResult::Action(FrontendAction::FpsShow),
        ClientCommand::NetStats => return CommandResult::Action(FrontendAction::NetStats),
        ClientCommand::State => return CommandResult::Action(FrontendAction::State),
        ClientCommand::Clear => return CommandResult::Action(FrontendAction::Clear),
        ClientCommand::Quit => return CommandResult::Action(FrontendAction::Quit),
        ClientCommand::SkyQuality(quality) => {
            return CommandResult::Action(FrontendAction::Property(ClientPropertyCommand::Set(
                ClientPropertyValue::SkyQuality(quality),
            )))
        }
        ClientCommand::SkyDemo(enabled) => {
            return CommandResult::Action(FrontendAction::Property(ClientPropertyCommand::Set(
                ClientPropertyValue::SkyDemo(enabled),
            )))
        }
        ClientCommand::Fov(Some(degrees)) => {
            return CommandResult::Action(FrontendAction::Property(ClientPropertyCommand::Set(
                ClientPropertyValue::CameraFov(degrees),
            )))
        }
        ClientCommand::Fov(None) => {
            return CommandResult::Action(FrontendAction::Property(ClientPropertyCommand::Get(
                ClientPropertyKey::CameraFov,
            )))
        }
        ClientCommand::Property(command) => {
            if let Some(clock) = ClockAction::from_property(&command) {
                return CommandResult::Action(FrontendAction::Clock(clock));
            }
            if let Some(result) = movement::handle_property(&command) {
                return result;
            }
            return CommandResult::Action(FrontendAction::Property(command));
        }
        ClientCommand::ShowCelestialNames(enabled) => {
            return CommandResult::Action(FrontendAction::Property(ClientPropertyCommand::Set(
                ClientPropertyValue::SkyNames(enabled),
            )))
        }
        ClientCommand::SetTime { minute, local } => {
            return CommandResult::Action(FrontendAction::Clock(ClockAction::Write(
                ClockWrite::HourMinute {
                    minute,
                    solar: local,
                },
            )))
        }
        ClientCommand::SetInstant(utc) => {
            return CommandResult::Action(FrontendAction::Clock(ClockAction::Write(
                ClockWrite::Set(utc),
            )))
        }
        ClientCommand::CameraLook { yaw, pitch } => {
            return CommandResult::Action(FrontendAction::CameraLook { yaw, pitch })
        }
        ClientCommand::CameraStatus => return CommandResult::Action(FrontendAction::CameraStatus),
        ClientCommand::InitialClockConfig => {
            return CommandResult::Action(FrontendAction::Clock(ClockAction::InitialConfig))
        }
        ClientCommand::TimeStatus => {
            return CommandResult::Action(FrontendAction::Clock(ClockAction::Status))
        }
        ClientCommand::TimeScale(value) => {
            return CommandResult::Action(FrontendAction::Clock(ClockAction::Scale(value)))
        }
        ClientCommand::SpawnRandomBolid(count) => {
            return CommandResult::Action(FrontendAction::SpawnRandomBolid(count))
        }
        ClientCommand::SpawnRandomMeteorid => {
            return CommandResult::Action(FrontendAction::SpawnRandomMeteorid)
        }
    };
    let result = (context.execute_action)(action);
    match result {
        LocalActionOutcome::Queued { kind, .. } => CommandResult::Queued { kind },
        LocalActionOutcome::Rejected { kind, error } => {
            CommandResult::QueueRejected { kind, error }
        }
        LocalActionOutcome::Suppressed(reason) => CommandResult::Suppressed(reason),
    }
}

#[cfg(test)]
mod tests;
