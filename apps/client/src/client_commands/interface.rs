use super::{ClientCommand, CommandRegistry, CommandSpec, ParseError};

const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "help",
        alias: None,
        usage: "help",
        parser: parse,
    },
    CommandSpec {
        name: "fps show",
        alias: None,
        usage: "fps show",
        parser: parse,
    },
    CommandSpec {
        name: "net stats",
        alias: None,
        usage: "net stats",
        parser: parse,
    },
    CommandSpec {
        name: "state",
        alias: None,
        usage: "state",
        parser: parse,
    },
    CommandSpec {
        name: "clear",
        alias: None,
        usage: "clear",
        parser: parse,
    },
    CommandSpec {
        name: "quit",
        alias: Some("exit"),
        usage: "quit (alias: exit)",
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
        ("fps show", "") => Some(ClientCommand::FpsShow),
        ("net stats", "") => Some(ClientCommand::NetStats),
        ("state", "") => Some(ClientCommand::State),
        ("clear", "") => Some(ClientCommand::Clear),
        ("quit", "") => Some(ClientCommand::Quit),
        _ => None,
    };
    result.ok_or(ParseError::UnknownCommand)
}
