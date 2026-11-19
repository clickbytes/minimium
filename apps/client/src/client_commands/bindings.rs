use super::catalog::catalog;
use super::{ClientCommand, ParseError};
use minimium_commands::operation::CatalogEntry;
use minimium_commands::syntax::bind::{bind_line, ParsedLine, PreparedOperation};
use minimium_commands::{CommandError, CommandErrorCode as Code, PropertyRequest};

pub(super) fn parse(
    input: &str,
    identity: &mut minimium_commands::results::OperationContext,
) -> Result<Option<ClientCommand>, CommandError> {
    match bind_line(catalog(), input)? {
        ParsedLine::Namespace(path) => Ok(Some(ClientCommand::Print(
            catalog()
                .children(&path)
                .into_iter()
                .collect::<Vec<_>>()
                .join(" "),
        ))),
        ParsedLine::Legacy => super::text::legacy(input),
        ParsedLine::Operation(op) => {
            let entry = catalog().resolve(&op.path).expect("bound operation");
            identity.operation = op.id.clone();
            identity.scope = entry.spec.scope.to_owned();
            if op.path == "ui.console.explain" {
                return super::explain::describe(&op.arguments[0].text)
                    .map(|s| Some(ClientCommand::Print(s)));
            }
            lower(&op, entry).map(Some)
        }
    }
}

pub(super) fn lower(
    op: &PreparedOperation,
    entry: &CatalogEntry,
) -> Result<ClientCommand, CommandError> {
    let span = op.arguments.first().map_or(op.span, |a| a.span);
    let invalid = |reason: &str| {
        CommandError::new(
            Code::InvalidValue,
            Some(&op.path),
            reason,
            Some(&op.spelling),
        )
        .at(span)
    };
    if op.path == "server.persistence.save" {
        return Ok(ClientCommand::SaveWorld);
    }
    if op.path == "world.time.diagnostics" {
        return Ok(ClientCommand::TimeStatus);
    }
    if op.path == "server.persistence.status" {
        let id = op
            .arguments
            .first()
            .filter(|arg| arg.text != "none")
            .map(|arg| {
                arg.text
                    .parse::<u128>()
                    .ok()
                    .filter(|id| *id != 0)
                    .ok_or_else(|| invalid("expected exact positive SaveJobId"))
            })
            .transpose()?;
        return Ok(ClientCommand::SaveStatus(id));
    }
    if op.path == "client.connection.connect"
        && !minimium_client_runtime::valid_endpoint(&op.arguments[0].text)
    {
        return Err(CommandError::new(
            Code::InvalidEndpoint,
            Some("endpoint"),
            "expected HOST:PORT",
            None::<String>,
        )
        .at(span));
    }
    if op.path == "client.camera.look" {
        use minimium_commands::operation::CommandValue;
        let [yaw, pitch] = op.arguments.as_slice() else {
            return Err(invalid("expected yaw and pitch in degrees"));
        };
        let (CommandValue::Finite(y), CommandValue::Finite(p)) = (&yaw.value, &pitch.value) else {
            return Err(invalid("expected finite degrees"));
        };
        return super::camera::look(y.get() as f32, p.get() as f32)
            .map_err(|reason| invalid(reason).at(pitch.span));
    }
    if op.path == "client.camera.status" {
        return Ok(ClientCommand::CameraStatus);
    }
    if op.path == "client.render.water.status" {
        return Ok(ClientCommand::Presentation(
            super::PresentationOperation::WaterStatus,
        ));
    }
    if op.path == "world.time.initial_config" {
        return Ok(ClientCommand::InitialClockConfig);
    }
    if matches!(
        op.path.as_str(),
        "client.render.bolids.transmission"
            | "client.sky.meteors.debug.mode"
            | "client.sky.meteors.debug.light"
    ) && !op.legacy
    {
        use minimium_commands::syntax::canonical::AccessKind;
        match (op.path.as_str(), op.kind) {
            ("client.render.bolids.transmission", AccessKind::Read) => {
                return Ok(ClientCommand::Presentation(
                    super::PresentationOperation::Developer(
                        super::presentation::DeveloperOperation::BolidTransmissionRead,
                    ),
                ))
            }
            ("client.render.bolids.transmission", AccessKind::Reset) => {
                return Ok(ClientCommand::Presentation(
                    super::PresentationOperation::Developer(
                        super::presentation::DeveloperOperation::BolidTransmission(false),
                    ),
                ))
            }
            ("client.render.bolids.transmission", AccessKind::Write) => {
                let enabled = matches!(
                    op.arguments.first().map(|arg| arg.text.as_str()),
                    Some("true")
                );
                return Ok(ClientCommand::Presentation(
                    super::PresentationOperation::Developer(
                        super::presentation::DeveloperOperation::BolidTransmission(enabled),
                    ),
                ));
            }
            ("client.sky.meteors.debug.mode", AccessKind::Read) => {
                return Ok(ClientCommand::Presentation(
                    super::PresentationOperation::MeteorDebugRead,
                ))
            }
            ("client.sky.meteors.debug.mode", AccessKind::Reset) => {
                return Ok(ClientCommand::Presentation(
                    super::PresentationOperation::MeteorDebug(super::MeteorDebug::Off),
                ))
            }
            ("client.sky.meteors.debug.mode", AccessKind::Write) => {
                let mode = op
                    .arguments
                    .first()
                    .and_then(|arg| super::MeteorDebug::parse(&arg.text))
                    .ok_or_else(|| invalid("expected off|light|occlusion|paths|budget"))?;
                return Ok(ClientCommand::Presentation(
                    super::PresentationOperation::MeteorDebug(mode),
                ));
            }
            ("client.sky.meteors.debug.light", AccessKind::Read) => {
                return Ok(ClientCommand::Presentation(
                    super::PresentationOperation::MeteorLightRead,
                ))
            }
            ("client.sky.meteors.debug.light", AccessKind::Reset) => {
                return Ok(ClientCommand::Presentation(
                    super::PresentationOperation::MeteorLight(super::MeteorLightDebug::On),
                ))
            }
            ("client.sky.meteors.debug.light", AccessKind::Write) => {
                let mode = match op.arguments.first().map(|arg| arg.text.as_str()) {
                    Some("on") => super::MeteorLightDebug::On,
                    Some("off") => super::MeteorLightDebug::Off,
                    Some("transmission") => super::MeteorLightDebug::Transmission,
                    _ => return Err(invalid("expected on|off|transmission")),
                };
                return Ok(ClientCommand::Presentation(
                    super::PresentationOperation::MeteorLight(mode),
                ));
            }
            _ => {}
        }
    }
    if op.kind == minimium_commands::syntax::canonical::AccessKind::Read {
        match op.path.as_str() {
            "client.render.bolids.transmission" => {
                return Ok(ClientCommand::Presentation(
                    super::PresentationOperation::Developer(
                        super::presentation::DeveloperOperation::BolidTransmissionRead,
                    ),
                ))
            }
            "client.sky.meteors.debug.mode" => {
                return Ok(ClientCommand::Presentation(
                    super::PresentationOperation::MeteorDebugRead,
                ))
            }
            "client.sky.meteors.debug.light" if !op.legacy => {
                return Ok(ClientCommand::Presentation(
                    super::PresentationOperation::MeteorLightRead,
                ))
            }
            _ => {}
        }
    }
    if !matches!(
        op.path.as_str(),
        "client.render.bolids.transmission"
            | "client.sky.meteors.debug.mode"
            | "client.sky.meteors.debug.light"
    ) {
        if let Some(request) = op.property_request(entry) {
            return super::properties::parse_request(request)
                .map_err(|e| e.at(span))?
                .map(ClientCommand::Property)
                .ok_or_else(|| invalid("unsupported property operation"));
        }
    }
    if op.kind == minimium_commands::syntax::canonical::AccessKind::Read
        && entry.spec.kind == minimium_commands::operation::OperationKind::Property
        && op.spelling != "sky meteors debug light"
    {
        return Err(CommandError::new(
            Code::UnsupportedContext,
            Some(&op.path),
            "legacy presentation backend has no read binding",
            None::<String>,
        )
        .at(op.span));
    }
    let name = entry
        .command
        .ok_or_else(|| invalid("operation has no client binding"))?;
    let values = op
        .arguments
        .iter()
        .map(|a| a.text.as_str())
        .collect::<Vec<_>>();
    match (name, values.as_slice()) {
        ("social say", [text]) if !minimium_protocol::valid_chat(text) => {
            return Err(invalid("chat requires 1..128 UTF-8 bytes without controls"))
        }
        ("social name", [name]) if !minimium_protocol::valid_name(name) => {
            return Err(invalid(
                "name requires 1..24 ASCII letters, digits or underscore",
            ))
        }
        ("social say", [text]) => return Ok(ClientCommand::Chat((*text).to_owned())),
        ("config save", [path]) if !op.legacy && !path.is_empty() => {
            return Ok(ClientCommand::ConfigSave((*path).into()))
        }
        ("help", []) => return Ok(ClientCommand::Help),
        ("help", [target]) => {
            return super::properties::parse_request(PropertyRequest::Help {
                target: Some((*target).to_owned()),
            })?
            .map(ClientCommand::Property)
            .ok_or_else(|| invalid("unknown help target"))
        }
        _ => {}
    }
    let mut args = values.join(" ");
    if let Some(option) = entry.legacy_option {
        if !args.is_empty() {
            args = format!("{option} {args}");
        }
    }
    if name.starts_with("dev ")
        && super::presentation::COMMAND_FORMS
            .iter()
            .any(|(form, _)| *form == name)
    {
        let mut words = name
            .split_whitespace()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        words.extend(values.iter().map(|v| {
            if name == "dev bolid transmission" {
                match *v {
                    "true" => "on",
                    "false" => "off",
                    other => other,
                }
                .to_owned()
            } else {
                (*v).to_owned()
            }
        }));
        return super::presentation::parse_developer(&words)
            .map(ClientCommand::Presentation)
            .map_err(|e| e.at(span));
    }
    let spec = super::command_specs()
        .iter()
        .find(|s| s.name == name)
        .ok_or_else(|| invalid("missing legacy parser"))?;
    (spec.parser)(&[name, &args]).map_err(|e| match e {
        ParseError::Structured(error) => error.at(span),
        _ => CommandError::new(
            Code::InvalidArguments,
            Some(&op.path),
            "invalid command arguments",
            Some(spec.usage),
        )
        .at(span),
    })
}
