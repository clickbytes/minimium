//! Shared typed command parsing and dispatch for terminal and graphical clients.
//!
//! [`parse_command`] accepts the union of the terminal and F1 console commands.
//! A startup registry is the source for help, aliases, and completion. Execution
//! queues protocol messages directly and reports UI/process work as a typed
//! [`FrontendAction`], leaving each front end in control of its own event loop.

use crate::{
    environment::{quality::SkyQuality, weather::WeatherCommand},
    options::ThreadLane,
    session::QueueError,
};
use minimium_protocol::ClientMessage;
use std::sync::OnceLock;

mod clouds;
mod connection;
mod environment;
mod gameplay;
mod interface;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ClientCommand {
    Help,
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
    Weather(WeatherCommand),
    TimeStatus,
    TimeScale(String),
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CommandSpec {
    pub name: &'static str,
    pub alias: Option<&'static str>,
    pub usage: &'static str,
    parser: fn(&[&str]) -> Result<ClientCommand, ParseError>,
}

pub(super) struct CommandRegistry {
    commands: Vec<CommandSpec>,
}

impl CommandRegistry {
    pub(super) fn register(&mut self, specs: &'static [CommandSpec]) {
        for spec in specs {
            for name in std::iter::once(spec.name).chain(spec.alias) {
                assert!(
                    !self.commands.iter().any(|existing| {
                        std::iter::once(existing.name)
                            .chain(existing.alias)
                            .any(|registered| registered == name)
                    }),
                    "duplicate console command: {name}"
                );
            }
            self.commands.push(*spec);
        }
    }

    fn parse(&self, input: &str, words: &[&str]) -> Option<Result<ClientCommand, ParseError>> {
        self.commands
            .iter()
            .filter_map(|spec| {
                let matched = [Some(spec.name), spec.alias]
                    .into_iter()
                    .flatten()
                    .map(|name| name.split_whitespace().collect::<Vec<_>>())
                    .filter(|name| words.starts_with(name))
                    .max_by_key(Vec::len)?;
                Some((spec, matched.len()))
            })
            .max_by_key(|(_, length)| *length)
            .map(|(spec, command_length)| {
                let arguments = [spec.name, tail_after_tokens(input, command_length)];
                (spec.parser)(&arguments)
            })
    }
}

fn tail_after_tokens(input: &str, count: usize) -> &str {
    let mut tail = input;
    for _ in 0..count {
        tail = tail.trim_start();
        let end = tail.find(char::is_whitespace).unwrap_or(tail.len());
        tail = &tail[end..];
    }
    tail.trim()
}

static REGISTRY: OnceLock<CommandRegistry> = OnceLock::new();

pub(crate) fn initialize_registry() {
    let _ = registry();
}

fn registry() -> &'static CommandRegistry {
    REGISTRY.get_or_init(|| {
        let mut registry = CommandRegistry {
            commands: Vec::new(),
        };
        interface::register(&mut registry);
        connection::register(&mut registry);
        gameplay::register(&mut registry);
        environment::register(&mut registry);
        registry
    })
}

pub(crate) fn help_lines() -> impl Iterator<Item = &'static str> {
    registry().commands.iter().map(|command| command.usage)
}

pub(crate) fn command_specs() -> &'static [CommandSpec] {
    &registry().commands
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
    WeatherUsage,
    TimeScaleUsage,
    UnknownCommand,
}

pub(crate) fn parse_command(input: &str) -> Result<Option<ClientCommand>, ParseError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let words = trimmed.split_whitespace().collect::<Vec<_>>();
    registry()
        .parse(trimmed, &words)
        .unwrap_or(Err(ParseError::UnknownCommand))
        .map(Some)
}

pub(crate) struct CommandContext<'a> {
    pub connection: &'a mut crate::connection_controller::ConnectionController,
    /// Direction used for `fire`; terminal callers pass [1, 0, 0].
    pub fire_direction: [f32; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum FrontendAction {
    Help,
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
    Weather(WeatherCommand),
    TimeStatus,
    TimeScale(String),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CommandResult {
    Sent,
    QueueError(QueueError),
    Action(FrontendAction),
}

pub(crate) fn execute(command: ClientCommand, context: &mut CommandContext<'_>) -> CommandResult {
    let message = match command {
        ClientCommand::Ping => ClientMessage::Ping,
        ClientCommand::Move(direction) => ClientMessage::Move {
            direction,
            jump: false,
        },
        ClientCommand::Jump => ClientMessage::Move {
            direction: [0.0; 2],
            jump: true,
        },
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
        ClientCommand::Weather(command) => {
            return CommandResult::Action(FrontendAction::Weather(command))
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
