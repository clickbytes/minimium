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
    if op.path == "world.time.initial_config" {
        return Ok(ClientCommand::InitialClockConfig);
    }
    if let Some(request) = op.property_request(entry) {
        return super::properties::parse_request(request)
            .map_err(|e| e.at(span))?
            .map(ClientCommand::Property)
            .ok_or_else(|| invalid("unsupported property operation"));
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
