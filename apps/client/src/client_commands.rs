//! Shared typed command parsing and dispatch for terminal and graphical clients.
//!
//! [`parse_command`] accepts the union of the terminal and F1 console commands.
//! [`COMMAND_CATALOG`] is the source for help, aliases, and completion. Execution
//! queues protocol messages directly and reports UI/process work as a typed
//! [`FrontendAction`], leaving each front end in control of its own event loop.

use crate::{options::ThreadLane, session::QueueError};
use minimium_protocol::ClientMessage;

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
    Fire,
    Chat(String),
    SetName(String),
    State,
    Clear,
    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CommandSpec {
    pub name: &'static str,
    pub alias: Option<&'static str>,
    pub usage: &'static str,
}

/// Canonical command names, aliases, and syntax used by help and completion.
pub(crate) const COMMAND_CATALOG: &[CommandSpec] = &[
    CommandSpec {
        name: "help",
        alias: None,
        usage: "help",
    },
    CommandSpec {
        name: "host local",
        alias: None,
        usage: "host local [--threads 1|2|auto]",
    },
    CommandSpec {
        name: "connect",
        alias: None,
        usage: "connect HOST:PORT",
    },
    CommandSpec {
        name: "disconnect",
        alias: None,
        usage: "disconnect",
    },
    CommandSpec {
        name: "fps show",
        alias: None,
        usage: "fps show",
    },
    CommandSpec {
        name: "net stats",
        alias: None,
        usage: "net stats",
    },
    CommandSpec {
        name: "ping",
        alias: None,
        usage: "ping",
    },
    CommandSpec {
        name: "move",
        alias: None,
        usage: "move X Z",
    },
    CommandSpec {
        name: "jump",
        alias: None,
        usage: "jump",
    },
    CommandSpec {
        name: "fire",
        alias: None,
        usage: "fire",
    },
    CommandSpec {
        name: "say",
        alias: Some("chat"),
        usage: "say TEXT (alias: chat)",
    },
    CommandSpec {
        name: "name",
        alias: None,
        usage: "name NAME",
    },
    CommandSpec {
        name: "state",
        alias: None,
        usage: "state",
    },
    CommandSpec {
        name: "clear",
        alias: None,
        usage: "clear",
    },
    CommandSpec {
        name: "quit",
        alias: Some("exit"),
        usage: "quit (alias: exit)",
    },
];

pub(crate) fn help_lines() -> impl Iterator<Item = &'static str> {
    COMMAND_CATALOG.iter().map(|command| command.usage)
}

pub(crate) fn completion_candidates() -> impl Iterator<Item = &'static str> {
    COMMAND_CATALOG
        .iter()
        .flat_map(|command| std::iter::once(command.name).chain(command.alias))
}

fn is_command(input: &str, canonical: &str) -> bool {
    COMMAND_CATALOG.iter().any(|command| {
        command.name == canonical && (input == command.name || command.alias == Some(input))
    })
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
    UnknownCommand,
}

pub(crate) fn parse_command(input: &str) -> Result<Option<ClientCommand>, ParseError> {
    let trimmed = input.trim();
    if let Some((verb, text)) = trimmed.split_once(char::is_whitespace) {
        if is_command(verb, "say") {
            let text = text.trim();
            return if text.is_empty() {
                Err(ParseError::MissingChat)
            } else {
                Ok(Some(ClientCommand::Chat(text.to_owned())))
            };
        }
    }
    let mut words = trimmed.split_whitespace();
    let parts = (
        words.next(),
        words.next(),
        words.next(),
        words.next(),
        words.next(),
    );
    let command = match parts {
        (None, None, None, None, None) => return Ok(None),
        (Some("help"), None, None, None, None) => ClientCommand::Help,
        (Some("host"), Some("local"), None, None, None) => ClientCommand::HostLocal(None),
        (Some("host"), Some("local"), Some("--threads"), Some(lane), None) => {
            ClientCommand::HostLocal(Some(
                ThreadLane::parse(lane).map_err(|_| ParseError::InvalidLane)?,
            ))
        }
        (Some("connect"), Some(address), None, None, None) => {
            ClientCommand::Connect(address.into())
        }
        (Some("disconnect"), None, None, None, None) => ClientCommand::Disconnect,
        (Some("fps"), Some("show"), None, None, None) => ClientCommand::FpsShow,
        (Some("net"), Some("stats"), None, None, None) => ClientCommand::NetStats,
        (Some("ping"), None, None, None, None) => ClientCommand::Ping,
        (Some("move"), Some(x), Some(z), None, None) => {
            ClientCommand::Move([parse_coordinate(x)?, parse_coordinate(z)?])
        }
        (Some("jump"), None, None, None, None) => ClientCommand::Jump,
        (Some("fire"), None, None, None, None) => ClientCommand::Fire,
        (Some("name"), Some(name), None, None, None) => ClientCommand::SetName(name.into()),
        (Some("state"), None, None, None, None) => ClientCommand::State,
        (Some("clear"), None, None, None, None) => ClientCommand::Clear,
        (Some(verb), None, None, None, None) if is_command(verb, "quit") => ClientCommand::Quit,
        (Some("host"), Some("local"), ..) => return Err(ParseError::HostUsage),
        (Some("connect"), ..) => return Err(ParseError::ConnectUsage),
        (Some("move"), ..) => return Err(ParseError::MoveUsage),
        (Some("name"), ..) => return Err(ParseError::NameUsage),
        (Some(verb), ..) if is_command(verb, "say") => return Err(ParseError::MissingChat),
        _ => return Err(ParseError::UnknownCommand),
    };
    Ok(Some(command))
}

fn parse_coordinate(value: &str) -> Result<f32, ParseError> {
    let coordinate = value
        .parse::<f32>()
        .map_err(|_| ParseError::InvalidCoordinate)?;
    if !coordinate.is_finite() {
        return Err(ParseError::NonFiniteCoordinate);
    }
    Ok(coordinate)
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
    Clear,
    Quit,
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
    };
    match context.connection.queue(&message) {
        Ok(()) => CommandResult::Sent,
        Err(error) => CommandResult::QueueError(error),
    }
}
