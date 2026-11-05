use super::{ClientCommand, CommandRegistry, CommandSpec, ParseError};
use crate::options::ThreadLane;

const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "network host local",
        alias: Some("host local"),
        usage: "network host local [--threads 1|2|auto]",
        parser: parse,
    },
    CommandSpec {
        name: "network connect",
        alias: Some("connect"),
        usage: "network connect HOST:PORT",
        parser: parse,
    },
    CommandSpec {
        name: "network disconnect",
        alias: Some("disconnect"),
        usage: "network disconnect",
        parser: parse,
    },
    CommandSpec {
        name: "network ping",
        alias: Some("ping"),
        usage: "network ping",
        parser: parse,
    },
];

pub(super) fn register(registry: &mut CommandRegistry) {
    registry.register(COMMANDS);
}

fn parse(words: &[&str]) -> Result<ClientCommand, ParseError> {
    let [command, raw_arguments] = words else {
        return Err(ParseError::UnknownCommand);
    };
    let arguments = raw_arguments.split_whitespace().collect::<Vec<_>>();
    match (*command, arguments.as_slice()) {
        ("network host local", []) => Ok(ClientCommand::HostLocal(None)),
        ("network host local", ["--threads", lane]) => Ok(ClientCommand::HostLocal(Some(
            ThreadLane::parse(lane).map_err(|_| ParseError::InvalidLane)?,
        ))),
        ("network host local", _) => Err(ParseError::HostUsage),
        ("network connect", [address]) => Ok(ClientCommand::Connect((*address).to_owned())),
        ("network connect", _) => Err(ParseError::ConnectUsage),
        ("network disconnect", []) => Ok(ClientCommand::Disconnect),
        ("network ping", []) => Ok(ClientCommand::Ping),
        _ => Err(ParseError::UnknownCommand),
    }
}
