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
enum CommandId {
    Help,
    HostLocal,
    Connect,
    Disconnect,
    FpsShow,
    NetStats,
    Ping,
    Move,
    Jump,
    Fire,
    Chat,
    SetName,
    State,
    Clear,
    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CommandSpec {
    id: CommandId,
    pub name: &'static str,
    pub alias: Option<&'static str>,
    pub usage: &'static str,
}

/// Canonical command names, aliases, and syntax used by help and completion.
pub(crate) const COMMAND_CATALOG: &[CommandSpec] = &[
    CommandSpec {
        id: CommandId::Help,
        name: "help",
        alias: None,
        usage: "help",
    },
    CommandSpec {
        id: CommandId::HostLocal,
        name: "host local",
        alias: None,
        usage: "host local [--threads 1|2|auto]",
    },
    CommandSpec {
        id: CommandId::Connect,
        name: "connect",
        alias: None,
        usage: "connect HOST:PORT",
    },
    CommandSpec {
        id: CommandId::Disconnect,
        name: "disconnect",
        alias: None,
        usage: "disconnect",
    },
    CommandSpec {
        id: CommandId::FpsShow,
        name: "fps show",
        alias: None,
        usage: "fps show",
    },
    CommandSpec {
        id: CommandId::NetStats,
        name: "net stats",
        alias: None,
        usage: "net stats",
    },
    CommandSpec {
        id: CommandId::Ping,
        name: "ping",
        alias: None,
        usage: "ping",
    },
    CommandSpec {
        id: CommandId::Move,
        name: "move",
        alias: None,
        usage: "move X Z",
    },
    CommandSpec {
        id: CommandId::Jump,
        name: "jump",
        alias: None,
        usage: "jump",
    },
    CommandSpec {
        id: CommandId::Fire,
        name: "fire",
        alias: None,
        usage: "fire",
    },
    CommandSpec {
        id: CommandId::Chat,
        name: "say",
        alias: Some("chat"),
        usage: "say TEXT (alias: chat)",
    },
    CommandSpec {
        id: CommandId::SetName,
        name: "name",
        alias: None,
        usage: "name NAME",
    },
    CommandSpec {
        id: CommandId::State,
        name: "state",
        alias: None,
        usage: "state",
    },
    CommandSpec {
        id: CommandId::Clear,
        name: "clear",
        alias: None,
        usage: "clear",
    },
    CommandSpec {
        id: CommandId::Quit,
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

fn command_id(first: &str, second: Option<&str>) -> Option<CommandId> {
    COMMAND_CATALOG
        .iter()
        .find(|spec| {
            if let Some((verb, subcommand)) = spec.name.split_once(' ') {
                first == verb && second == Some(subcommand)
            } else {
                first == spec.name || spec.alias == Some(first)
            }
        })
        .map(|spec| spec.id)
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
        if command_id(verb, None) == Some(CommandId::Chat) {
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
    let id = parts.0.and_then(|first| command_id(first, parts.1));
    let command = match (id, parts) {
        (_, (None, None, None, None, None)) => return Ok(None),
        (Some(CommandId::Help), (_, None, None, None, None)) => ClientCommand::Help,
        (Some(CommandId::HostLocal), (_, _, None, None, None)) => ClientCommand::HostLocal(None),
        (Some(CommandId::HostLocal), (_, _, Some("--threads"), Some(lane), None)) => {
            ClientCommand::HostLocal(Some(
                ThreadLane::parse(lane).map_err(|_| ParseError::InvalidLane)?,
            ))
        }
        (Some(CommandId::Connect), (_, Some(address), None, None, None)) => {
            ClientCommand::Connect(address.into())
        }
        (Some(CommandId::Disconnect), (_, None, None, None, None)) => ClientCommand::Disconnect,
        (Some(CommandId::FpsShow), (_, _, None, None, None)) => ClientCommand::FpsShow,
        (Some(CommandId::NetStats), (_, _, None, None, None)) => ClientCommand::NetStats,
        (Some(CommandId::Ping), (_, None, None, None, None)) => ClientCommand::Ping,
        (Some(CommandId::Move), (_, Some(x), Some(z), None, None)) => {
            ClientCommand::Move([parse_coordinate(x)?, parse_coordinate(z)?])
        }
        (Some(CommandId::Jump), (_, None, None, None, None)) => ClientCommand::Jump,
        (Some(CommandId::Fire), (_, None, None, None, None)) => ClientCommand::Fire,
        (Some(CommandId::SetName), (_, Some(name), None, None, None)) => {
            ClientCommand::SetName(name.into())
        }
        (Some(CommandId::State), (_, None, None, None, None)) => ClientCommand::State,
        (Some(CommandId::Clear), (_, None, None, None, None)) => ClientCommand::Clear,
        (Some(CommandId::Quit), (_, None, None, None, None)) => ClientCommand::Quit,
        (Some(CommandId::HostLocal), _) => return Err(ParseError::HostUsage),
        (Some(CommandId::Connect), _) => return Err(ParseError::ConnectUsage),
        (Some(CommandId::Move), _) => return Err(ParseError::MoveUsage),
        (Some(CommandId::SetName), _) => return Err(ParseError::NameUsage),
        (Some(CommandId::Chat), _) => return Err(ParseError::MissingChat),
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
