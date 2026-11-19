use super::{catalog, parse_server_spec, validate_property_request, ServerCommand};
use minimium_commands::operation::{CatalogEntry, OperationCatalog};
use minimium_commands::syntax::bind::{bind_line, ParsedLine, PreparedOperation};
use minimium_commands::{CommandError, CommandErrorCode as Code};

pub(crate) fn parse(line: &str, catalog: &OperationCatalog) -> Result<ServerCommand, CommandError> {
    if line.len() > 512 || line.chars().any(char::is_control) {
        return Err(CommandError::new(
            Code::InvalidArguments,
            Some("command"),
            "invalid admin command length or control character",
            None::<String>,
        ));
    }
    match bind_line(catalog, line)? {
        ParsedLine::Legacy => ServerCommand::parse_legacy(line),
        ParsedLine::Namespace(name) => Ok(ServerCommand::CatalogOutput(
            catalog
                .children(&name)
                .into_iter()
                .collect::<Vec<_>>()
                .join(" "),
        )),
        ParsedLine::Operation(op) => {
            let entry = catalog.resolve(&op.path).expect("catalog binding");
            if op.path == "ui.console.explain" {
                let report = match bind_line(catalog, &op.arguments[0].text)? {
                    ParsedLine::Namespace(name) => format!("namespace {name}"),
                    ParsedLine::Legacy => match ServerCommand::parse_legacy(&op.arguments[0].text)?
                    {
                        ServerCommand::UnresolvedModule(_) => {
                            return Err(CommandError::new(
                                Code::UnknownCommand,
                                Some(&op.arguments[0].text),
                                "unknown operation",
                                Some("help"),
                            )
                            .at(op.arguments[0].span))
                        }
                        request => format!("legacy request {request:?}"),
                    },
                    ParsedLine::Operation(inner) => {
                        let spec = catalog.resolve(&inner.path).expect("catalog binding");
                        if inner.path != "ui.console.explain" {
                            let _ = lower(&inner, spec)?;
                        }
                        inner.explanation(spec).to_string()
                    }
                };
                Ok(ServerCommand::CatalogOutput(report))
            } else {
                lower(&op, entry)
            }
        }
    }
}

fn lower(op: &PreparedOperation, entry: &CatalogEntry) -> Result<ServerCommand, CommandError> {
    let span = op.arguments.first().map_or(op.span, |arg| arg.span);
    if let Some(request) = op.property_request(entry) {
        return validate_property_request(request).map_err(|err| err.at(span));
    }
    let name = entry.command.ok_or_else(|| {
        CommandError::new(
            Code::UnsupportedContext,
            Some(&op.path),
            "no administrative handler",
            None::<String>,
        )
        .at(op.span)
    })?;
    let args = op
        .arguments
        .iter()
        .map(|arg| arg.text.clone())
        .collect::<Vec<_>>();
    if name == "help" {
        return Ok(if let Some(target) = args.first() {
            ServerCommand::Property(super::ServerPropertyCommand::Help(Some(target.clone())))
        } else {
            ServerCommand::Help
        });
    }
    if let Some(spec) = super::server_registry()
        .commands()
        .iter()
        .find(|spec| spec.name == name)
    {
        if args.len() > spec.arguments.len() {
            return Err(CommandError::new(
                Code::UnsupportedContext,
                Some(&op.path),
                "legacy backend cannot set this argument",
                Some(spec.usage),
            )
            .at(span));
        }
        return parse_server_spec(&[name, &args.join(" ")]).map_err(|err| err.at(span));
    }
    Ok(ServerCommand::Registered {
        identity: name.into(),
        arguments: args,
    })
}

/// Standalone embedders can parse built-ins before constructing the runtime.
/// Installed extensions are resolved by the runtime's own catalog at its text inputs.
pub(super) fn standalone(line: &str) -> Result<ServerCommand, CommandError> {
    use std::sync::OnceLock;
    static BUILTINS: OnceLock<OperationCatalog> = OnceLock::new();
    let catalog = BUILTINS.get_or_init(|| {
        let modules =
            crate::commands::initialize_modules(&[]).expect("built-in module declarations");
        catalog::build(&modules).expect("built-in operation declarations")
    });
    parse(line, catalog)
}
