use super::*;

pub(super) fn parse_property(line: &str) -> Result<ServerCommand, CommandError> {
    let usage = "get KEY | set KEY VALUE | reset KEY | list [PREFIX] | help [COMMAND|KEY]";
    let request = parse_property_request(line)?.ok_or_else(|| {
        parse_error(
            CommandErrorCode::InvalidArguments,
            "property command",
            "invalid property command",
            usage,
        )
    })?;
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
                    if !(10..=3600).contains(&minimum) || !(minimum..=3600).contains(&maximum) {
                        return Err(property_error(
                            &key,
                            CommandErrorCode::InvalidValue,
                            "range must be ascending and within 10..=3600 seconds",
                            usage,
                        ));
                    }
                    Some((minimum, maximum))
                };
                return Ok(ServerCommand::Property(
                    ServerPropertyCommand::SetBolidInterval(interval),
                ));
            }
            let seconds = value.parse::<u64>().map_err(|_| {
                property_error(
                    &key,
                    CommandErrorCode::InvalidValue,
                    "autosave interval must be an integer from 0 through 604800",
                    usage,
                )
            })?;
            if seconds > 604_800 {
                return Err(property_error(
                    &key,
                    CommandErrorCode::InvalidValue,
                    "autosave interval must be an integer from 0 through 604800",
                    usage,
                ));
            }
            Ok(ServerCommand::Property(ServerPropertyCommand::SetAutosave(
                seconds,
            )))
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
            if let Some(target) = target.as_deref() {
                if property_registry().get(target).is_none()
                    && server_command_usage(target).is_none()
                {
                    return Err(property_error(
                        target,
                        CommandErrorCode::HelpQuery,
                        format!("unknown command or server property `{target}`"),
                        usage,
                    ));
                }
            }
            Ok(ServerCommand::Property(ServerPropertyCommand::Help(target)))
        }
    }
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
