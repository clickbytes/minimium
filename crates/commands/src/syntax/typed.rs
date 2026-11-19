use super::bind::{prepare, PreparedOperation};
use super::canonical::{Literal, Statement};
use crate::admin::AdminPayload;
use crate::operation::{CommandValue, OperationCatalog};
use crate::{CommandError, CommandErrorCode as Code, SourceSpan};

/// Bind a structured request directly to its catalog entry. No command line is reconstructed.
pub fn bind_operation(
    catalog: &OperationCatalog,
    payload: &AdminPayload,
) -> Result<PreparedOperation, CommandError> {
    let AdminPayload::Operation {
        operation_id,
        access,
        arguments,
        value,
    } = payload
    else {
        return Err(CommandError::new(
            Code::InvalidArguments,
            Some("operation"),
            "expected typed invocation",
            None::<String>,
        ));
    };
    let error =
        |code, reason| CommandError::new(code, Some(&operation_id.0), reason, None::<String>);
    let entry = catalog
        .resolve(&operation_id.0)
        .filter(|e| e.spec.id == *operation_id)
        .ok_or_else(|| error(Code::UnknownCommand, "unknown operation identity"))?;
    let supplied: Vec<_> = arguments.iter().chain(value.iter()).collect();
    let span = SourceSpan { start: 0, end: 0 };
    let literals = supplied
        .iter()
        .map(|arg| {
            let (text, quoted) = match arg {
                CommandValue::Unit => ("none".into(), false),
                CommandValue::Text(s) | CommandValue::Enum(s) => (s.clone(), true),
                CommandValue::Record(_) | CommandValue::Page { .. } => {
                    return Err(error(Code::InvalidValue, "expected scalar input"))
                }
                _ => (arg.encode(), false),
            };
            Ok(Literal { text, quoted, span })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let operation = prepare(
        entry,
        Statement {
            name: operation_id.0.clone(),
            span,
            access: *access,
            arguments: literals,
        },
        false,
    )
    .map_err(|mut error| {
        error.span = None;
        error
    })?;
    if !operation
        .arguments
        .iter()
        .zip(supplied)
        .all(|(bound, supplied)| &bound.value == supplied)
    {
        return Err(error(
            Code::InvalidValue,
            "value tag does not match operation schema",
        ));
    }
    Ok(operation)
}
