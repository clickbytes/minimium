use std::sync::OnceLock;

use minimium_commands::{CommandRegistry, CommandSpec};
use minimium_environment::{TimeCommand, TIME_USAGE};

const SERVER_SPECS: &[CommandSpec<ServerCommand, String>] = &[
    CommandSpec {
        name: "help",
        alias: None,
        usage: "help",
        parser: parse_server_spec,
    },
    CommandSpec {
        name: "status",
        alias: None,
        usage: "status",
        parser: parse_server_spec,
    },
    CommandSpec {
        name: "players list",
        alias: None,
        usage: "players list",
        parser: parse_server_spec,
    },
    CommandSpec {
        name: "metrics show",
        alias: None,
        usage: "metrics show",
        parser: parse_server_spec,
    },
    CommandSpec {
        name: "world save",
        alias: None,
        usage: "world save",
        parser: parse_server_spec,
    },
    CommandSpec {
        name: "stop",
        alias: None,
        usage: "stop",
        parser: parse_server_spec,
    },
    CommandSpec {
        name: "time",
        alias: None,
        usage: TIME_USAGE,
        parser: parse_server_spec,
    },
    CommandSpec {
        name: "time status",
        alias: None,
        usage: "time status",
        parser: parse_server_spec,
    },
    CommandSpec {
        name: "time mode real_time",
        alias: None,
        usage: "time mode real_time",
        parser: parse_server_spec,
    },
    CommandSpec {
        name: "time mode time_scale",
        alias: None,
        usage: "time mode time_scale",
        parser: parse_server_spec,
    },
    CommandSpec {
        name: "time set",
        alias: None,
        usage: "time set RFC3339_UTC",
        parser: parse_server_spec,
    },
    CommandSpec {
        name: "time scale",
        alias: None,
        usage: "time scale DECIMAL",
        parser: parse_server_spec,
    },
    CommandSpec {
        name: "time offline pause",
        alias: None,
        usage: "time offline pause",
        parser: parse_server_spec,
    },
    CommandSpec {
        name: "time offline advance",
        alias: None,
        usage: "time offline advance",
        parser: parse_server_spec,
    },
];

static SERVER_REGISTRY: OnceLock<CommandRegistry<ServerCommand, String>> = OnceLock::new();

fn server_registry() -> &'static CommandRegistry<ServerCommand, String> {
    SERVER_REGISTRY.get_or_init(|| {
        let mut registry = CommandRegistry::default();
        registry.register(SERVER_SPECS);
        registry
    })
}

fn parse_server_spec(arguments: &[&str]) -> Result<ServerCommand, String> {
    let [command, tail] = arguments else {
        return Err(String::from("invalid command"));
    };
    if command.starts_with("time") {
        let joined = format!("{command} {tail}");
        let words = joined.split_whitespace().skip(1).collect::<Vec<_>>();
        return TimeCommand::parse(&words).map(ServerCommand::Time);
    }
    if !tail.is_empty() {
        return Err(String::from("command takes no arguments"));
    }
    match *command {
        "help" => Ok(ServerCommand::Help),
        "status" => Ok(ServerCommand::Status),
        "players list" => Ok(ServerCommand::Players),
        "metrics show" => Ok(ServerCommand::Metrics),
        "world save" => Ok(ServerCommand::Save),
        "stop" => Ok(ServerCommand::Stop),
        _ => Err(String::from("invalid command")),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerCommand {
    Help,
    Status,
    Players,
    Metrics,
    Save,
    Stop,
    Time(TimeCommand),
    /// A validated administrative line owned by its registering server module.
    Registered(String),
}

impl ServerCommand {
    pub fn parse(line: &str) -> Result<Self, String> {
        let words: Vec<_> = line.split_whitespace().collect();
        if words.is_empty() {
            return Err("empty command".into());
        }
        if let Some(parsed) = server_registry().parse(line, &words) {
            match parsed {
                Ok(command) => return Ok(command),
                Err((_, error)) if words[0] == "time" => return Err(error),
                Err(_) => {}
            }
        }
        (line.len() <= 512 && line.chars().all(|c| !c.is_control()))
            .then(|| Self::Registered(line.trim().to_owned()))
            .ok_or_else(|| "invalid command; use `help`".into())
    }

    pub const fn is_status(&self) -> bool {
        matches!(self, Self::Status)
    }

    pub const fn is_metrics(&self) -> bool {
        matches!(self, Self::Metrics)
    }

    pub const fn is_save_or_stop(&self) -> bool {
        matches!(self, Self::Save | Self::Stop)
    }
}

pub fn server_help() -> String {
    server_registry()
        .commands()
        .iter()
        .filter(|spec| spec.name != "time")
        .map(|spec| spec.usage)
        .collect::<Vec<_>>()
        .join(" | ")
}
