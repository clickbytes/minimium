use super::{ClientCommand, CommandRegistry, CommandSpec, ParseError};
use crate::options::ThreadLane;

const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "host local",
        alias: None,
        usage: "host local [--threads 1|2|auto]",
        parser: parse,
    },
    CommandSpec {
        name: "connect",
        alias: None,
        usage: "connect HOST:PORT",
        parser: parse,
    },
    CommandSpec {
        name: "disconnect",
        alias: None,
        usage: "disconnect",
        parser: parse,
    },
    CommandSpec {
        name: "ping",
        alias: None,
        usage: "ping",
        parser: parse,
    },
];

pub(super) fn register(registry: &mut CommandRegistry) {
    registry.register(COMMANDS);
}

fn parse(words: &[&str]) -> Result<ClientCommand, ParseError> {
    match words {
        ["host", "local"] => Ok(ClientCommand::HostLocal(None)),
        ["host", "local", "--threads", lane] => Ok(ClientCommand::HostLocal(Some(
            ThreadLane::parse(lane).map_err(|_| ParseError::InvalidLane)?,
        ))),
        ["host", "local", ..] => Err(ParseError::HostUsage),
        ["connect", address] => Ok(ClientCommand::Connect((*address).to_owned())),
        ["connect", ..] => Err(ParseError::ConnectUsage),
        ["disconnect"] => Ok(ClientCommand::Disconnect),
        ["ping"] => Ok(ClientCommand::Ping),
        _ => Err(ParseError::UnknownCommand),
    }
}
