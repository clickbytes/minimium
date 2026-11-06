use super::{ClientCommand, CommandRegistry, CommandSpec, ParseError};
use std::path::PathBuf;

const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "help",
        alias: None,
        usage: "help",
        parser: parse,
    },
    CommandSpec {
        name: "config save",
        alias: None,
        usage: "config save PATH",
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
    if *command == "config save" {
        let path = arguments.trim();
        let path = if path.starts_with('"') && path.ends_with('"') && path.len() >= 2 {
            &path[1..path.len() - 1]
        } else if path.split_whitespace().count() == 1 {
            path
        } else {
            return Err(ParseError::Usage("config save PATH"));
        };
        if path.is_empty() {
            return Err(ParseError::Usage("config save PATH"));
        }
        return Ok(ClientCommand::ConfigSave(PathBuf::from(path)));
    }
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
