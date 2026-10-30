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
    match words {
        ["help"] => Ok(ClientCommand::Help),
        ["fps", "show"] => Ok(ClientCommand::FpsShow),
        ["net", "stats"] => Ok(ClientCommand::NetStats),
        ["state"] => Ok(ClientCommand::State),
        ["clear"] => Ok(ClientCommand::Clear),
        ["quit"] | ["exit"] => Ok(ClientCommand::Quit),
        _ => Err(ParseError::UnknownCommand),
    }
}
