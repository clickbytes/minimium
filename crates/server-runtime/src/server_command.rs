use std::sync::OnceLock;

use minimium_commands::{
    lex_command_line, parse_property_request, CommandError, CommandErrorCode, CommandOutcome,
    CommandRegistry, CommandSpec, PropertyLifecycle, PropertyPermission, PropertyRegistry,
    PropertyRequest, PropertySpec, PropertyType,
};
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
static PROPERTY_REGISTRY: OnceLock<PropertyRegistry> = OnceLock::new();
mod properties;
use properties::SERVER_PROPERTIES;

pub(crate) const SERVER_COMMAND_NAMES: &[&str] = &[
    "help",
    "status",
    "players list",
    "metrics show",
    "world save",
    "stop",
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
    Property(ServerPropertyCommand),
    /// A validated administrative line owned by its registering server module.
    Registered(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerPropertyCommand {
    Get(String),
    SetAutosave(u64),
    ResetAutosave,
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

pub(crate) fn server_command_usage(target: &str) -> Option<&'static str> {
    server_registry()
        .commands()
        .iter()
        .find(|spec| spec.name == target || spec.alias == Some(target))
        .map(|spec| spec.usage)
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
        let normalized = words.join(" ");
        if matches!(words[0], "get" | "set" | "reset" | "list")
            || (words[0] == "help" && words.len() > 1)
        {
            return parse_property(line.trim())
                .map_err(|error| typed_property_error(&normalized, error));
        }
        match server_registry().parse_outcome(&normalized, &words) {
            CommandOutcome::Matched {
                result: Ok(command),
                ..
            } => return Ok(command),
            CommandOutcome::Matched {
                name,
                usage,
                result: Err(error),
            } => {
                return Err(parse_error(
                    CommandErrorCode::InvalidArguments,
                    name,
                    error,
                    usage,
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

fn parse_property(line: &str) -> Result<ServerCommand, String> {
    let usage = "get KEY | set KEY VALUE | reset KEY | list [PREFIX] | help [COMMAND|KEY]";
    let request = parse_property_request(line)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "invalid property command".to_owned())?;
    match request {
        PropertyRequest::Get { key } => {
            require_property(&key, usage)?;
            Ok(ServerCommand::Property(ServerPropertyCommand::Get(key)))
        }
        PropertyRequest::Set { key, value } => {
            let spec = require_property(&key, usage)?;
            if key.starts_with("world.time.") {
                let command = match key.as_str() {
                    "world.time.mode" => TimeCommand::parse(&["mode", value.as_str()]),
                    "world.time.scale" => TimeCommand::parse(&["scale", value.as_str()]),
                    "world.time.offline_policy" => TimeCommand::parse(&["offline", value.as_str()]),
                    _ => return Err(format!("{key} is read-only")),
                }
                .map_err(|error| format!("{key}: {error}"))?;
                return Ok(ServerCommand::Time(command));
            }
            if key != "server.autosave_interval" || spec.lifecycle != PropertyLifecycle::Live {
                return Err(format!("{key} is read-only during this server session"));
            }
            let seconds = value.parse::<u64>().map_err(|_| {
                "autosave interval must be an integer from 0 through 604800".to_owned()
            })?;
            if seconds > 604_800 {
                return Err("autosave interval must be an integer from 0 through 604800".into());
            }
            Ok(ServerCommand::Property(ServerPropertyCommand::SetAutosave(
                seconds,
            )))
        }
        PropertyRequest::Reset { key } => {
            let spec = require_property(&key, usage)?;
            if key != "server.autosave_interval" || !spec.resettable {
                return Err(format!("{key} cannot be reset during this server session"));
            }
            Ok(ServerCommand::Property(
                ServerPropertyCommand::ResetAutosave,
            ))
        }
        PropertyRequest::List { prefix } => {
            Ok(ServerCommand::Property(ServerPropertyCommand::List(prefix)))
        }
        PropertyRequest::Help { target } => {
            if let Some(target) = target.as_deref() {
                if property_registry().get(target).is_none()
                    && server_command_usage(target).is_none()
                {
                    return Err(format!("unknown command or server property `{target}`"));
                }
            }
            Ok(ServerCommand::Property(ServerPropertyCommand::Help(target)))
        }
    }
}

fn require_property(key: &str, usage: &str) -> Result<&'static PropertySpec, String> {
    property_registry()
        .get(key)
        .ok_or_else(|| format!("unknown server property `{key}`; use `list server.` ({usage})"))
}

fn typed_property_error(line: &str, message: String) -> CommandError {
    let code = if message.contains("unknown command or server property") {
        CommandErrorCode::HelpQuery
    } else if message.contains("unknown server property") {
        CommandErrorCode::UnknownProperty
    } else if message.contains("requires a property key") || message.contains("requires a value") {
        CommandErrorCode::MissingValue
    } else if message.contains("read-only") || message.contains("cannot be reset") {
        CommandErrorCode::UnsupportedContext
    } else if message.contains("invalid arguments") {
        CommandErrorCode::InvalidArguments
    } else {
        CommandErrorCode::InvalidValue
    };
    let reason = message
        .split("\nusage:")
        .next()
        .unwrap_or(&message)
        .split("; use `list server.`")
        .next()
        .unwrap_or(&message)
        .to_owned();
    CommandError::new(
        code,
        property_key(line),
        reason,
        Some("get KEY | set KEY VALUE | reset KEY | list [PREFIX] | help [COMMAND|KEY]"),
    )
}

fn property_key(line: &str) -> Option<String> {
    let words = lex_command_line(line).ok()?;
    words.get(1).cloned()
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
