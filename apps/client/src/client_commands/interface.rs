use super::{ClientCommand, CommandRegistry, CommandSpec, ParseError};

const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "help",
        alias: None,
        usage: "help",
        parser: parse,
    },
    CommandSpec {
        name: "diagnostics fps",
        alias: Some("fps show"),
        usage: "diagnostics fps",
        parser: parse,
    },
    CommandSpec {
        name: "network stats",
        alias: Some("net stats"),
        usage: "network stats",
        parser: parse,
    },
    CommandSpec {
        name: "diagnostics state",
        alias: Some("state"),
        usage: "diagnostics state",
        parser: parse,
    },
    CommandSpec {
        name: "console clear",
        alias: Some("clear"),
        usage: "console clear",
        parser: parse,
    },
    CommandSpec {
        name: "console quit",
        alias: Some("exit"),
        usage: "console quit (alias: exit)",
        parser: parse,
    },
];

pub(super) fn register(registry: &mut CommandRegistry) {
    registry.register(COMMANDS);
}

fn parse(words: &[&str]) -> Result<ClientCommand, ParseError> {
    let [command, arguments] = words else {
        return Err(ParseError::UnknownCommand);
    };
    let result = match (*command, *arguments) {
        ("help", "") => Some(ClientCommand::Help),
        ("diagnostics fps", "") => Some(ClientCommand::FpsShow),
        ("network stats", "") => Some(ClientCommand::NetStats),
        ("diagnostics state", "") => Some(ClientCommand::State),
        ("console clear", "") => Some(ClientCommand::Clear),
        ("console quit", "") => Some(ClientCommand::Quit),
        _ => None,
    };
    result.ok_or(ParseError::UnknownCommand)
}
