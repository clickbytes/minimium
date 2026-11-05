//! Shared typed command parsing and dispatch for terminal and graphical clients.
//!
//! [`parse_command`] accepts the union of the terminal and F1 console commands.
//! A startup registry is the source for help, aliases, and completion. Execution
//! queues protocol messages directly and reports UI/process work as a typed
//! [`FrontendAction`], leaving each front end in control of its own event loop.

use crate::{options::ThreadLane, session::QueueError};
use minimium_commands::{
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
mod weather;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ClientCommand {
    Movement(movement::Setting),
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
    WeatherStatus,
    WeatherDebug(minimium_environment::weather::CloudDebug),
    WeatherDetail(u8),
}

pub(crate) type CommandSpec = SharedCommandSpec<ClientCommand, ParseError>;
pub(super) type CommandRegistry = SharedCommandRegistry<ClientCommand, ParseError>;

static REGISTRY: OnceLock<CommandRegistry> = OnceLock::new();

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

pub(crate) fn help_lines() -> impl Iterator<Item = &'static str> {
    registry().commands().iter().map(|command| command.usage)
}

pub(crate) fn command_specs() -> &'static [CommandSpec] {
    registry().commands()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    UnknownCommand,
    Usage(&'static str),
}

pub(crate) fn parse_command(input: &str) -> Result<Option<ClientCommand>, ParseError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let words = trimmed.split_whitespace().collect::<Vec<_>>();
    let group: Vec<_> = command_specs()
        .iter()
        .filter(|spec| spec.name.starts_with(&format!("{trimmed} ")))
        .map(|spec| spec.usage)
        .collect();
    if !group.is_empty() {
        return Ok(Some(ClientCommand::Print(group.join("\n"))));
    }
    registry()
        .parse(trimmed, &words)
        .map(|result| result.map_err(|(usage, _)| ParseError::Usage(usage)))
        .unwrap_or_else(|| {
            let domain = words[0];
            let choices: Vec<_> = command_specs()
                .iter()
                .filter(|spec| spec.name.split_whitespace().next() == Some(domain))
                .map(|spec| spec.usage)
                .collect();
            Ok(ClientCommand::Print(format!(
                "Unknown command. Available usage:\n{}",
                if choices.is_empty() {
                    help_lines().collect::<Vec<_>>().join("\n")
                } else {
                    choices.join("\n")
                }
            )))
        })
        .map(Some)
}

pub(crate) struct CommandContext<'a> {
    pub connection: &'a mut minimium_client_runtime::ConnectionController,
    /// Direction used for `fire`; terminal callers pass [1, 0, 0].
    pub fire_direction: [f32; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum FrontendAction {
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
    SkyQuality(SkyQuality),
    SkyInfo,
    SkyBodies,
    SkyDemo(bool),
    Fov(Option<f32>),
    ShowCelestialNames(bool),
    TimeStatus,
    TimeScale(String),
    WeatherStatus,
    WeatherDebug(minimium_environment::weather::CloudDebug),
    WeatherDetail(u8),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CommandResult {
    Sent,
    QueueError(QueueError),
    Action(FrontendAction),
}

pub(crate) fn execute(command: ClientCommand, context: &mut CommandContext<'_>) -> CommandResult {
    let message = match command {
        ClientCommand::Movement(setting) => return movement::execute(setting, context),
        ClientCommand::Print(text) => return CommandResult::Action(FrontendAction::Print(text)),
        ClientCommand::WeatherStatus => {
            return CommandResult::Action(FrontendAction::WeatherStatus)
        }
        ClientCommand::WeatherDebug(mode) => {
            return CommandResult::Action(FrontendAction::WeatherDebug(mode))
        }
        ClientCommand::WeatherDetail(level) => {
            return CommandResult::Action(FrontendAction::WeatherDetail(level))
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
            return CommandResult::Action(FrontendAction::SkyQuality(quality))
        }
        ClientCommand::SkyInfo => return CommandResult::Action(FrontendAction::SkyInfo),
        ClientCommand::SkyBodies => return CommandResult::Action(FrontendAction::SkyBodies),
        ClientCommand::SkyDemo(enabled) => {
            return CommandResult::Action(FrontendAction::SkyDemo(enabled))
        }
        ClientCommand::Fov(degrees) => return CommandResult::Action(FrontendAction::Fov(degrees)),
        ClientCommand::ShowCelestialNames(enabled) => {
            return CommandResult::Action(FrontendAction::ShowCelestialNames(enabled))
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
