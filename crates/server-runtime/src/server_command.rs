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
        name: "spawn_random_meteorid",
        alias: None,
        usage: "spawn_random_meteorid",
        arguments: &[],
        parser: parse_server_spec,
    },
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
        usage: "players list [CURSOR]",
        arguments: &[CommandArgumentSpec::new(
            "cursor",
            CommandArgumentType::Text,
            true,
            false,
            None,
            None,
            None,
            "opaque continuation cursor",
        )],
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
        usage: "spawn_random_bolid [N: 1..64]",
        arguments: &[CommandArgumentSpec::new(
            "count",
            CommandArgumentType::Integer,
            true,
            false,
            None,
            Some("1..64"),
            Some("bolids"),
            "number of bolids, default 1",
        )],
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
            "requested global simulation speed (zero pauses gameplay)",
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

pub(crate) mod bindings;
pub(crate) mod catalog;
mod identity;

pub(crate) fn reserved_command_names() -> Vec<&'static str> {
    server_registry()
        .commands()
        .iter()
        .flat_map(|spec| std::iter::once(spec.name).chain(spec.alias))
        .chain(["get", "set", "reset", "list"])
        .collect()
}

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
        let time_arguments = command
            .split_whitespace()
            .skip(1)
            .chain(tail.split_whitespace())
            .collect::<Vec<_>>();
        return TimeCommand::parse(&time_arguments)
            .and_then(|command| validate_time_command(command).map_err(str::to_owned))
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
    if *command == "spawn_random_meteorid" && tail.is_empty() {
        return Ok(ServerCommand::SpawnRandomMeteorid);
    }
    if *command == "spawn_random_bolid" {
        let count = if tail.is_empty() {
            Some(1)
        } else {
            tail.parse::<u16>().ok()
        };
        return validate_bolid_count(count).map(ServerCommand::SpawnRandomBolid);
    }
    if *command == "players list" {
        if tail.is_empty() {
            return Ok(ServerCommand::Players(None));
        }
        if tail.len() <= 256 && !tail.chars().any(char::is_control) {
            return Ok(ServerCommand::Players(Some((*tail).to_owned())));
        }
        return Err(parse_error(
            CommandErrorCode::InvalidArguments,
            "players list",
            "cursor exceeds its bound",
            "players list [CURSOR]",
        ));
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
        "metrics show" => Ok(ServerCommand::Metrics),
        "world save" => Ok(ServerCommand::Save),
        "stop" => Ok(ServerCommand::Stop),
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
    CatalogOutput {
        operation: &'static str,
        text: String,
    },
    Help,
    Status,
    Players(Option<String>),
    Metrics,
    Save,
    Stop,
    SpawnRandomBolid(u16),
    SpawnRandomMeteorid,
    Time(TimeCommand),
    Property(ServerPropertyCommand),
    /// A validated administrative line owned by its registering server module.
    Registered {
        identity: String,
        arguments: Vec<String>,
    },
    UnresolvedModule(Vec<String>),
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

pub(crate) fn validate_property_request(
    request: PropertyRequest,
) -> Result<ServerCommand, CommandError> {
    property_parsing::validate_property_request(request)
}

pub(crate) fn set_autosave_interval(seconds: u64) -> Result<ServerCommand, CommandError> {
    property_parsing::set_autosave_interval(seconds)
}

pub(crate) fn set_bolid_interval(
    interval: Option<(u64, u64)>,
) -> Result<ServerCommand, CommandError> {
    property_parsing::set_bolid_interval(interval)
}

pub(crate) fn validate_bolid_count(count: Option<u16>) -> Result<u16, CommandError> {
    count
        .filter(|count| (1..=64).contains(count))
        .ok_or_else(|| {
            parse_error(
                CommandErrorCode::InvalidValue,
                "count",
                "expected an integer from 1 to 64",
                "spawn_random_bolid [N: 1..64]",
            )
        })
}

pub(crate) fn validate_time_command(command: TimeCommand) -> Result<TimeCommand, &'static str> {
    match command {
        TimeCommand::Scale(rate) if rate.get() != 0 && rate.get() < 100_000 => Err(
            "rate must be 0 (pause) or between 0.1 and 1000000 with at most 6 fractional digits",
        ),
        TimeCommand::Set(instant)
            if instant.get() < minimium_environment::time::MIN_WORLD_TIME_MS
                || instant.get() >= minimium_environment::time::MAX_WORLD_TIME_MS_EXCLUSIVE =>
        {
            Err("UTC date is outside the supported world-time range")
        }
        _ => Ok(command),
    }
}

impl ServerCommand {
    pub fn parse(line: &str) -> Result<Self, CommandError> {
        bindings::standalone(line)
    }

    fn parse_legacy(line: &str) -> Result<Self, CommandError> {
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
        match server_registry().parse_outcome(line, &words) {
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
            Ok(Self::UnresolvedModule(owned_words))
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
