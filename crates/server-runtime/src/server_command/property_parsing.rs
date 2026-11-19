use super::*;

const PROPERTY_USAGE: &str =
    "get KEY | set KEY VALUE | reset KEY | list [PREFIX] | help [COMMAND|KEY]";

pub(super) fn parse_property(line: &str) -> Result<ServerCommand, CommandError> {
    let request = parse_property_request(line)?.ok_or_else(|| {
        parse_error(
            CommandErrorCode::InvalidArguments,
            "property command",
            "invalid property command",
            PROPERTY_USAGE,
        )
    })?;
    validate_property_request(request)
}

pub(super) fn validate_property_request(
    request: PropertyRequest,
) -> Result<ServerCommand, CommandError> {
    let usage = PROPERTY_USAGE;
    if property_request_size(&request) > 512 || property_request_has_control(&request) {
        return Err(parse_error(
            CommandErrorCode::InvalidArguments,
            "property command",
            "property command exceeds 512 bytes or contains a control character",
            usage,
        ));
    }
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
                    _ => {
                        return Err(property_error(
                            &key,
                            CommandErrorCode::ReadOnly,
                            format!("{key} is read-only"),
                            usage,
                        ))
                    }
                }
                .map_err(|reason| {
                    property_error(&key, CommandErrorCode::InvalidValue, reason, usage)
                })?;
                let command = super::validate_time_command(command).map_err(|reason| {
                    property_error(&key, CommandErrorCode::InvalidValue, reason, usage)
                })?;
                return Ok(ServerCommand::Time(command));
            }
            if !matches!(
                key.as_str(),
                "server.autosave_interval" | "server.sky.bolid_interval"
            ) || spec.lifecycle != PropertyLifecycle::Live
            {
                let (code, message) = match spec.lifecycle {
                    PropertyLifecycle::CreationOnly => (
                        CommandErrorCode::CreationOnly,
                        format!("{key} is creation-only"),
                    ),
                    PropertyLifecycle::RestartRequired => (
                        CommandErrorCode::RestartRequired,
                        format!("{key} requires restart"),
                    ),
                    PropertyLifecycle::ReadOnly | PropertyLifecycle::Live => (
                        CommandErrorCode::ReadOnly,
                        format!("{key} is read-only during this server session"),
                    ),
                };
                return Err(property_error(&key, code, message, usage));
            }
            if key == "server.sky.bolid_interval" {
                let interval = if value == "off" {
                    None
                } else {
                    let (minimum, maximum) = value.split_once("..").ok_or_else(|| {
                        property_error(
                            &key,
                            CommandErrorCode::InvalidValue,
                            "expected off or MIN..MAX seconds",
                            usage,
                        )
                    })?;
                    let minimum = minimum.parse::<u64>().map_err(|_| {
                        property_error(
                            &key,
                            CommandErrorCode::InvalidValue,
                            "invalid minimum seconds",
                            usage,
                        )
                    })?;
                    let maximum = maximum.parse::<u64>().map_err(|_| {
                        property_error(
                            &key,
                            CommandErrorCode::InvalidValue,
                            "invalid maximum seconds",
                            usage,
                        )
                    })?;
                    Some((minimum, maximum))
                };
                return set_bolid_interval(interval);
            }
            let seconds = value.parse::<u64>().map_err(|_| {
                property_error(
                    &key,
                    CommandErrorCode::InvalidValue,
                    "autosave interval must be an integer from 0 through 604800",
                    usage,
                )
            })?;
            set_autosave_interval(seconds)
        }
        PropertyRequest::Reset { key } => {
            let spec = require_property(&key, usage)?;
            if !matches!(
                key.as_str(),
                "server.autosave_interval" | "server.sky.bolid_interval"
            ) || !spec.resettable
            {
                let (code, message) = match spec.lifecycle {
                    PropertyLifecycle::CreationOnly => (
                        CommandErrorCode::CreationOnly,
                        format!("{key} is creation-only"),
                    ),
                    PropertyLifecycle::RestartRequired => (
                        CommandErrorCode::RestartRequired,
                        format!("{key} requires restart"),
                    ),
                    _ => (
                        CommandErrorCode::ReadOnly,
                        format!("{key} cannot be reset during this server session"),
                    ),
                };
                return Err(property_error(&key, code, message, usage));
            }
            Ok(ServerCommand::Property(
                if key == "server.sky.bolid_interval" {
                    ServerPropertyCommand::ResetBolidInterval
                } else {
                    ServerPropertyCommand::ResetAutosave
                },
            ))
        }
        PropertyRequest::List { prefix } => {
            Ok(ServerCommand::Property(ServerPropertyCommand::List(prefix)))
        }
        PropertyRequest::Help { target } => {
            // Installed module names are available on the runtime owner.
            Ok(ServerCommand::Property(ServerPropertyCommand::Help(target)))
        }
    }
}

fn property_request_size(request: &PropertyRequest) -> usize {
    match request {
        PropertyRequest::Get { key } | PropertyRequest::Reset { key } => key.len(),
        PropertyRequest::Set { key, value } => key.len().saturating_add(value.len()),
        PropertyRequest::List { prefix } => prefix.as_ref().map_or(0, String::len),
        PropertyRequest::Help { target } => target.as_ref().map_or(0, String::len),
    }
}

fn property_request_has_control(request: &PropertyRequest) -> bool {
    let has_control = |value: &str| value.chars().any(char::is_control);
    match request {
        PropertyRequest::Get { key } | PropertyRequest::Reset { key } => has_control(key),
        PropertyRequest::Set { key, value } => has_control(key) || has_control(value),
        PropertyRequest::List { prefix } => prefix.as_deref().is_some_and(has_control),
        PropertyRequest::Help { target } => target.as_deref().is_some_and(has_control),
    }
}

pub(super) fn set_autosave_interval(seconds: u64) -> Result<ServerCommand, CommandError> {
    if seconds > 604_800 {
        return Err(property_error(
            "server.autosave_interval",
            CommandErrorCode::InvalidValue,
            "autosave interval must be an integer from 0 through 604800",
            PROPERTY_USAGE,
        ));
    }
    Ok(ServerCommand::Property(ServerPropertyCommand::SetAutosave(
        seconds,
    )))
}

pub(super) fn set_bolid_interval(
    interval: Option<(u64, u64)>,
) -> Result<ServerCommand, CommandError> {
    if interval.is_some_and(|(minimum, maximum)| {
        !(10..=3600).contains(&minimum) || !(minimum..=3600).contains(&maximum)
    }) {
        return Err(property_error(
            "server.sky.bolid_interval",
            CommandErrorCode::InvalidValue,
            "range must be ascending and within 10..=3600 seconds",
            PROPERTY_USAGE,
        ));
    }
    Ok(ServerCommand::Property(
        ServerPropertyCommand::SetBolidInterval(interval),
    ))
}

fn require_property(key: &str, usage: &str) -> Result<&'static PropertySpec, CommandError> {
    property_registry().get(key).ok_or_else(|| {
        property_error(
            key,
            CommandErrorCode::UnknownProperty,
            format!("unknown server property `{key}`; use `list server.`"),
            usage,
        )
    })
}

fn property_error(
    key: &str,
    code: CommandErrorCode,
    reason: impl Into<String>,
    usage: &str,
) -> CommandError {
    CommandError::new(code, Some(key), reason, Some(usage))
}
