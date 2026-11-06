//! Shared typed command parsing and dispatch for terminal and graphical clients.
//!
//! [`parse_command`] accepts the union of the terminal and F1 console commands.
//! A startup registry is the source for help, aliases, and completion. Execution
//! queues protocol messages directly and reports UI/process work as a typed
//! [`FrontendAction`], leaving each front end in control of its own event loop.

use crate::{options::ThreadLane, session::QueueError};
use minimium_commands::{
    lex_command_line_with_spans, CommandError, CommandErrorCode, CommandOutcome,
    CommandRegistry as SharedCommandRegistry, CommandSpec as SharedCommandSpec,
};
use minimium_protocol::ClientMessage;
use minimium_renderer::quality::SkyQuality;
use std::sync::OnceLock;

mod connection;
mod environment;
mod gameplay;
mod interface;
mod movement;
mod properties;
mod weather;

pub(crate) use properties::{
    format_property_help, format_time_value, parse_setting_value, property_spec, property_specs,
    ClientPropertyCommand, ClientPropertyKey, ClientPropertyValue,
};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ClientCommand {
    AtmosphereDebug(minimium_renderer::atmosphere_debug::AtmosphereDebug),
    AtmosphereStatus,
    TerrainDebug(minimium_renderer::terrain_debug::TerrainDebug),
    TerrainStatus,
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
    UsePortal,
    MapTeleport,
    Fire,
    Chat(String),
    SetName(String),
    State,
    Clear,
    Quit,
    SkyQuality(SkyQuality),
    SkyInfo,
    SkyBodies,
    SkyDemo(bool),
    Fov(Option<f32>),
    ShowCelestialNames(bool),
    TimeStatus,
    TimeScale(String),
    SetTime { minute: u16, local: bool },
    WeatherStatus,
    WeatherDebug(minimium_environment::weather::CloudDebug),
    WeatherDetail(u8),
}

pub(crate) type CommandSpec = SharedCommandSpec<ClientCommand, ParseError>;
pub(super) type CommandRegistry = SharedCommandRegistry<ClientCommand, ParseError>;

static REGISTRY: OnceLock<CommandRegistry> = OnceLock::new();

pub(crate) fn parse_fov_degrees(value: &str) -> Result<f32, &'static str> {
    let degrees = value.parse::<f32>().map_err(|_| "expected a number")?;
    if !degrees.is_finite() || !(30.0..=110.0).contains(&degrees) {
        return Err("field of view must be finite and between 30 and 110 degrees");
    }
    Ok(degrees)
}

pub(crate) fn initialize_registry() {
    let _ = registry();
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

pub(crate) fn help_lines() -> Vec<String> {
    let mut lines = registry()
        .commands()
        .iter()
        .map(|command| command.usage.to_owned())
        .collect::<Vec<_>>();
    lines.push("get KEY | set KEY VALUE | reset KEY | list [PREFIX] | help [COMMAND|KEY]".into());
    lines.extend(properties::property_specs().map(properties::format_property_help));
    lines
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
        .map(|spec| spec.usage)
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
                ParseError::UnknownCommand => ParseError::Usage(usage),
                error => error,
            })
            .map(Some),
        CommandOutcome::Unknown => {
            let domain = words[0];
            let choices: Vec<_> = command_specs()
                .iter()
                .filter(|spec| spec.name.split_whitespace().next() == Some(domain))
                .map(|spec| spec.usage)
                .collect();
            let choices = if choices.is_empty() {
                help_lines().join("\n")
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

pub(crate) fn parse_command_typed(input: &str) -> Result<Option<ClientCommand>, CommandError> {
    parse_command(input).map_err(|error| match error {
        ParseError::Structured(error) => error,
        ParseError::UnknownCommand => CommandError::new(
            CommandErrorCode::UnknownCommand,
            input.split_whitespace().next(),
            "unknown command",
            Some("help"),
        ),
        ParseError::FovUsage => CommandError::new(
            CommandErrorCode::InvalidValue,
            Some("client.camera.fov"),
            "field of view must be finite and between 30 and 110 degrees",
            Some("camera fov [30..=110]"),
        ),
        ParseError::Usage(usage) => CommandError::new(
            CommandErrorCode::InvalidArguments,
            input.split_whitespace().next(),
            "invalid command arguments",
            Some(usage),
        ),
        other => CommandError::new(
            CommandErrorCode::InvalidArguments,
            input.split_whitespace().next(),
            format!("{other:?}"),
            None::<String>,
        ),
    })
}

pub(crate) struct CommandContext<'a> {
    pub connection: &'a mut minimium_client_runtime::ConnectionController,
    /// Direction used for `fire`; terminal callers pass [1, 0, 0].
    pub fire_direction: [f32; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum FrontendAction {
    AtmosphereStatus,
    TerrainStatus,
    Help,
    Print(String),
    Connect(String),
    HostLocal(Option<ThreadLane>),
    Disconnect,
    FpsShow,
    NetStats,
    State,
    MapTeleport,
    Clear,
    Quit,
    SkyInfo,
    SkyBodies,
    Property(properties::ClientPropertyCommand),
    ConfigSave(std::path::PathBuf),
    TimeStatus,
    TimeScale(String),
    TimeMode(String),
    TimeOfflinePolicy(String),
    SetTime { minute: u16, local: bool },
    WeatherStatus,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CommandResult {
    Sent,
    QueueError(QueueError),
    Action(FrontendAction),
}

pub(crate) fn execute(command: ClientCommand, context: &mut CommandContext<'_>) -> CommandResult {
    let message = match command {
        ClientCommand::AtmosphereDebug(mode) => {
            return CommandResult::Action(FrontendAction::Property(ClientPropertyCommand::Set(
                ClientPropertyValue::AtmosphereDebug(mode),
            )))
        }
        ClientCommand::AtmosphereStatus => {
            return CommandResult::Action(FrontendAction::AtmosphereStatus)
        }
        ClientCommand::SetAltitude(km) => {
            if !context.connection.movement.fly {
                return CommandResult::Action(FrontendAction::Print(
                    "set_altitude requires fly on".into(),
                ));
            }
            ClientMessage::SetAltitude { km }
        }
        ClientCommand::ConfigSave(path) => {
            return CommandResult::Action(FrontendAction::ConfigSave(path))
        }
        ClientCommand::Print(text) => return CommandResult::Action(FrontendAction::Print(text)),
        ClientCommand::WeatherStatus => {
            return CommandResult::Action(FrontendAction::WeatherStatus)
        }
        ClientCommand::TerrainStatus => {
            return CommandResult::Action(FrontendAction::TerrainStatus)
        }
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
        ClientCommand::Ping => ClientMessage::Ping,
        ClientCommand::Move(direction) => {
            ClientMessage::Motion(minimium_simulation::CharacterInput {
                direction,
                movement: context.connection.movement,
                ..Default::default()
            })
        }
        ClientCommand::Jump => ClientMessage::Motion(minimium_simulation::CharacterInput {
            jump: true,
            vertical: 1.0,
            movement: context.connection.movement,
            ..Default::default()
        }),
        ClientCommand::UsePortal => ClientMessage::UsePortal,
        ClientCommand::MapTeleport => return CommandResult::Action(FrontendAction::MapTeleport),
        ClientCommand::Fire => ClientMessage::Fire {
            direction: context.fire_direction,
        },
        ClientCommand::Chat(text) => ClientMessage::Chat(text),
        ClientCommand::SetName(name) => ClientMessage::SetName(name),
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
        ClientCommand::SkyInfo => return CommandResult::Action(FrontendAction::SkyInfo),
        ClientCommand::SkyBodies => return CommandResult::Action(FrontendAction::SkyBodies),
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
            if let ClientPropertyCommand::Set(value) = &command {
                match value {
                    ClientPropertyValue::WorldTimeMode(mode) => {
                        return CommandResult::Action(FrontendAction::TimeMode(
                            match mode {
                                minimium_environment::time::ClockMode::RealTime => "real_time",
                                minimium_environment::time::ClockMode::TimeScale => "time_scale",
                            }
                            .to_owned(),
                        ))
                    }
                    ClientPropertyValue::WorldTimeScale(value) => {
                        return CommandResult::Action(FrontendAction::TimeScale(value.clone()))
                    }
                    ClientPropertyValue::WorldTimeOfflinePolicy(policy) => {
                        return CommandResult::Action(FrontendAction::TimeOfflinePolicy(
                            match policy {
                                minimium_environment::time::OfflinePolicy::Pause => "pause",
                                minimium_environment::time::OfflinePolicy::Advance => "advance",
                            }
                            .to_owned(),
                        ))
                    }
                    _ => {}
                }
            }
            if let Some(result) = movement::handle_property(command.clone(), context) {
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
            return CommandResult::Action(FrontendAction::SetTime { minute, local })
        }
        ClientCommand::TimeStatus => return CommandResult::Action(FrontendAction::TimeStatus),
        ClientCommand::TimeScale(value) => {
            return CommandResult::Action(FrontendAction::TimeScale(value))
        }
    };
    match context.connection.queue(&message) {
        Ok(()) => CommandResult::Sent,
        Err(error) => CommandResult::QueueError(error),
    }
}

#[cfg(test)]
mod tests {
    use super::{command_specs, parse_command, ClientCommand, ParseError};

    #[test]
    fn portal_is_available_to_the_text_console_and_completion() {
        assert_eq!(parse_command("portal"), Ok(Some(ClientCommand::UsePortal)));
        assert!(command_specs().iter().any(|spec| spec.name == "portal"));
    }

    #[test]
    fn aliases_resolve_through_registered_parsers_and_admin_mutation_is_not_a_client_command() {
        assert_eq!(
            parse_command("chat hello"),
            Ok(Some(ClientCommand::Chat("hello".into())))
        );
        assert_eq!(
            parse_command("timescale 1"),
            Ok(Some(ClientCommand::TimeScale("1".into())))
        );
        assert_eq!(
            parse_command("time mode real_time"),
            Err(ParseError::UnknownCommand)
        );
    }
}
