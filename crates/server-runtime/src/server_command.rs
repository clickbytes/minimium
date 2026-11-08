use std::sync::OnceLock;

use minimium_commands::{
    lex_command_line, parse_property_request, CommandArgumentSpec, CommandArgumentType,
    CommandError, CommandErrorCode, CommandOutcome, CommandRegistry, CommandSpec,
    PropertyLifecycle, PropertyPermission, PropertyRegistry, PropertyRequest, PropertySpec,
    PropertyType,
};
use minimium_environment::{TimeCommand, TIME_USAGE};

const SERVER_SPECS: &[CommandSpec<ServerCommand, CommandError>] = &[
    CommandSpec {
        name: "help",
        alias: None,
        usage: "help",
        arguments: &[],
        parser: parse_server_spec,
    },
    CommandSpec {
        name: "status",
        alias: None,
        usage: "status",
        arguments: &[],
        parser: parse_server_spec,
    },
    CommandSpec {
        name: "players list",
        alias: None,
        usage: "players list",
        arguments: &[],
        parser: parse_server_spec,
    },
    CommandSpec {
        name: "metrics show",
        alias: None,
        usage: "metrics show",
        arguments: &[],
        parser: parse_server_spec,
    },
    CommandSpec {
        name: "world save",
        alias: None,
        usage: "world save",
        arguments: &[],
        parser: parse_server_spec,
    },
    CommandSpec {
        name: "stop",
        alias: None,
        usage: "stop",
        arguments: &[],
        parser: parse_server_spec,
    },
    CommandSpec {
        name: "spawn_random_bolid",
        alias: None,
        usage: "spawn_random_bolid",
        arguments: &[],
        parser: parse_server_spec,
    },
    CommandSpec {
        name: "time",
        alias: None,
        usage: TIME_USAGE,
        arguments: &[],
        parser: parse_server_spec,
    },
    CommandSpec {
        name: "time status",
        alias: None,
        usage: "time status",
        arguments: &[],
        parser: parse_server_spec,
    },
    CommandSpec {
        name: "time mode real_time",
        alias: None,
        usage: "time mode real_time",
        arguments: &[],
        parser: parse_server_spec,
    },
    CommandSpec {
        name: "time mode time_scale",
        alias: None,
        usage: "time mode time_scale",
        arguments: &[],
        parser: parse_server_spec,
    },
    CommandSpec {
        name: "time set",
        alias: None,
        usage: "time set RFC3339_UTC",
        arguments: &[CommandArgumentSpec::new(
            "utc",
            CommandArgumentType::Rfc3339Utc,
            false,
            false,
            None,
            Some("RFC3339 with Z offset"),
            Some("UTC"),
            "set the world clock instant",
        )],
        parser: parse_server_spec,
    },
    CommandSpec {
        name: "time scale",
        alias: None,
        usage: "time scale DECIMAL",
        arguments: &[CommandArgumentSpec::new(
            "scale",
            CommandArgumentType::Decimal,
            false,
            false,
            None,
            Some("0 or 0.1..=1000000"),
            Some("x"),
            "world clock speed multiplier",
        )],
        parser: parse_server_spec,
    },
    CommandSpec {
        name: "time offline pause",
        alias: None,
        usage: "time offline pause",
        arguments: &[],
        parser: parse_server_spec,
    },
    CommandSpec {
        name: "time offline advance",
        alias: None,
        usage: "time offline advance",
        arguments: &[],
        parser: parse_server_spec,
    },
];

static SERVER_REGISTRY: OnceLock<CommandRegistry<ServerCommand, CommandError>> = OnceLock::new();
static PROPERTY_REGISTRY: OnceLock<PropertyRegistry> = OnceLock::new();
mod properties;
mod property_parsing;
use properties::SERVER_PROPERTIES;
use property_parsing::parse_property;

pub(crate) const SERVER_COMMAND_NAMES: &[&str] = &[
    "help",
    "status",
    "players list",
    "metrics show",
    "world save",
    "stop",
    "spawn_random_bolid",
    "time status",
    "time mode real_time",
    "time mode time_scale",
    "time set",
    "time scale",
    "time offline pause",
    "time offline advance",
    "get",
    "set",
    "reset",
    "list",
];

fn server_registry() -> &'static CommandRegistry<ServerCommand, CommandError> {
    SERVER_REGISTRY.get_or_init(|| {
        let mut registry = CommandRegistry::default();
        registry.register(SERVER_SPECS);
        registry
    })
}

fn parse_server_spec(arguments: &[&str]) -> Result<ServerCommand, CommandError> {
    let [command, tail] = arguments else {
        return Err(parse_error(
            CommandErrorCode::InvalidArguments,
            "command",
            "invalid command arguments",
            "help",
        ));
    };
    if command.starts_with("time") {
        let joined = format!("{command} {tail}");
        let words = joined.split_whitespace().skip(1).collect::<Vec<_>>();
        return TimeCommand::parse(&words)
            .map(ServerCommand::Time)
            .map_err(|reason| {
                parse_error(
                    CommandErrorCode::InvalidArguments,
                    "time",
                    reason,
                    TIME_USAGE,
                )
            });
    }
    if !tail.is_empty() {
        return Err(parse_error(
            CommandErrorCode::InvalidArguments,
            *command,
            "command takes no arguments",
            command,
        ));
    }
    match *command {
        "help" => Ok(ServerCommand::Help),
        "status" => Ok(ServerCommand::Status),
        "players list" => Ok(ServerCommand::Players),
        "metrics show" => Ok(ServerCommand::Metrics),
        "world save" => Ok(ServerCommand::Save),
        "stop" => Ok(ServerCommand::Stop),
        "spawn_random_bolid" => Ok(ServerCommand::SpawnRandomBolid),
        _ => Err(parse_error(
            CommandErrorCode::UnknownCommand,
            *command,
            "unknown command",
            "help",
        )),
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
    SpawnRandomBolid,
    Time(TimeCommand),
    Property(ServerPropertyCommand),
    /// A validated administrative line owned by its registering server module.
    Registered(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerPropertyCommand {
    Get(String),
    SetAutosave(u64),
    ResetAutosave,
    SetBolidInterval(Option<(u64, u64)>),
    ResetBolidInterval,
    List(Option<String>),
    Help(Option<String>),
}

const fn property(
    key: &'static str,
    value_type: PropertyType,
    default: &'static str,
    constraints: &'static str,
    lifecycle: PropertyLifecycle,
    resettable: bool,
) -> PropertySpec {
    PropertySpec::new(
        key,
        value_type,
        None,
        "Server startup or live operational setting.",
        Some(default),
        Some(constraints),
        "crates/server-runtime",
        PropertyPermission::ServerAdmin,
        lifecycle,
        false,
        resettable,
    )
}

fn property_registry() -> &'static PropertyRegistry {
    PROPERTY_REGISTRY.get_or_init(|| {
        let mut registry = PropertyRegistry::default();
        registry
            .register(SERVER_PROPERTIES)
            .expect("server property keys are unique");
        registry
    })
}

pub(crate) fn server_properties() -> impl Iterator<Item = &'static PropertySpec> {
    property_registry().specs()
}

pub(crate) fn server_command_usage(target: &str) -> Option<String> {
    server_registry()
        .commands()
        .iter()
        .find(|spec| spec.name == target || spec.alias == Some(target))
        .map(CommandSpec::format_help)
}

impl ServerCommand {
    pub fn parse(line: &str) -> Result<Self, CommandError> {
        let owned_words = lex_command_line(line)?;
        let words: Vec<_> = owned_words.iter().map(String::as_str).collect();
        if words.is_empty() {
            return Err(parse_error(
                CommandErrorCode::InvalidArguments,
                "command",
                "empty command",
                "help",
            ));
        }
        if matches!(words[0], "get" | "set" | "reset" | "list")
            || (words[0] == "help" && words.len() > 1)
        {
            return parse_property(line.trim());
        }
        match server_registry().parse_line(line)? {
            CommandOutcome::Matched {
                result: Ok(command),
                ..
            } => return Ok(command),
            CommandOutcome::Matched {
                name,
                usage,
                result: Err(error),
            } => {
                return Err(CommandError::new(
                    error.code,
                    error.subject.or_else(|| Some(name.to_owned())),
                    error.reason,
                    error.usage.or_else(|| Some(usage.to_owned())),
                ))
            }
            CommandOutcome::Unknown => {}
        }
        if line.len() <= 512 && line.chars().all(|c| !c.is_control()) {
            Ok(Self::Registered(line.trim().to_owned()))
        } else {
            Err(parse_error(
                CommandErrorCode::InvalidArguments,
                "command",
                "command exceeds 512 bytes or contains a control character",
                "help",
            ))
        }
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

fn parse_error(
    code: CommandErrorCode,
    subject: &str,
    reason: impl Into<String>,
    usage: &str,
) -> CommandError {
    CommandError::new(code, Some(subject), reason, Some(usage))
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
