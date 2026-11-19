use crate::server_command::ServerCommand;
use minimium_commands::admin::{AdminPayload, Correlation};
use minimium_commands::operation::OperationCatalog;
use minimium_commands::results::{OperationFailure, OperationReceipt, OperationResult, Stage};
use minimium_commands::{CommandError, CommandErrorCode};
use minimium_transport::{AdminInput, AdminRequestId, AdminResponse};
mod administration;
mod pages;
mod projection;

pub(crate) type RequestId = AdminRequestId;

pub(crate) fn response(
    reply: &crate::commands::CommandReply,
    correlation: Option<Correlation>,
    command: &ServerCommand,
    catalog: &OperationCatalog,
) -> AdminResponse {
    let view = reply.view();
    let text = crate::command_output::render_reply_view(&view)
        .as_str()
        .to_owned();
    let mut typed = correlation.map(|c| {
        let mut identity = command.identity(c.request_id, catalog);
        identity.correlation_nonce = Some(c.nonce);
        let projected = reply.view().operation_result(identity);
        let outcome = projected
            .outcome
            .map(|v| projection::payload(&v, command, catalog));
        OperationResult::new(projected.identity, projected.receipt, outcome)
    });
    if typed
        .as_ref()
        .is_some_and(|result| result.validate().is_err())
    {
        let result = typed.take().expect("validated above");
        typed = Some(OperationResult::new(
            result.identity,
            OperationReceipt::at(Stage::Rejected),
            Err(OperationFailure {
                code: CommandErrorCode::ResponseTooLarge.as_str().into(),
                subject: None,
                reason: "admin result exceeds the typed response budget".into(),
                usage: None,
                span: None,
                origin: "ServerRuntime".into(),
            }),
        ));
        return AdminResponse {
            typed,
            success: false,
            error_code: Some(CommandErrorCode::ResponseTooLarge.as_str().into()),
            key: None,
            text: "admin result exceeds the typed response budget".into(),
            usage: None,
        };
    }
    AdminResponse {
        typed,
        success: view.state == crate::hosting::replies::ReplyState::Executed,
        error_code: view.error_code.map(str::to_owned),
        key: view.error_subject.map(str::to_owned),
        text,
        usage: None,
    }
}

pub(crate) fn parse(
    input: &AdminInput,
    catalog: &OperationCatalog,
) -> Result<ServerCommand, AdminResponse> {
    let parsed = match input {
        AdminInput::Legacy(line) => crate::server_command::bindings::parse(line, catalog),
        AdminInput::Typed(request) => request.encode().and_then(|_| {
            let command = match &request.payload {
                AdminPayload::Text { text } => {
                    crate::server_command::bindings::parse(text, catalog)
                }
                payload => minimium_commands::syntax::typed::bind_operation(catalog, payload)
                    .and_then(|op| crate::server_command::bindings::from_prepared(op, catalog)),
            }?;
            match (command, request.actor, request.expected_context.as_ref()) {
                (
                    ServerCommand::TimeOfDay {
                        minute,
                        solar: true,
                        ..
                    },
                    Some(actor),
                    Some(context),
                ) => Ok(ServerCommand::TimeOfDay {
                    minute,
                    solar: true,
                    observer: Some((minimium_core::ids::PlayerId::new(actor), context.clone())),
                }),
                (command, None, None) => Ok(command),
                _ => Err(CommandError::new(
                    CommandErrorCode::StaleContext,
                    Some("expected_context"),
                    "expected an observer and world context for a solar clock request",
                    None::<String>,
                )),
            }
        }),
    };
    parsed.map_err(|error| AdminResponse {
        typed: match input {
            AdminInput::Typed(r) => Some(r.correlation.failure(
                r.operation(),
                Stage::Rejected,
                "admin_input",
                error.clone(),
            )),
            _ => None,
        },
        success: false,
        error_code: Some(error.code.as_str().into()),
        key: error.subject,
        text: error.reason,
        usage: error.usage,
    })
}

pub(crate) fn refuse(input: &AdminInput, code: &str, reason: &str, stage: Stage) -> AdminResponse {
    let typed = if let AdminInput::Typed(request) = input {
        let error = minimium_commands::results::OperationFailure {
            code: code.to_owned(),
            subject: None,
            reason: reason.to_owned(),
            usage: None,
            span: None,
            origin: "server_admission".into(),
        };
        Some(OperationResult::new(
            request.correlation.identity(request.operation()),
            minimium_commands::results::OperationReceipt::at(stage),
            Err(error),
        ))
    } else {
        None
    };
    AdminResponse {
        typed,
        success: false,
        error_code: Some(code.to_owned()),
        key: None,
        text: reason.to_owned(),
        usage: None,
    }
}
