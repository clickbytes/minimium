use minimium_transport::{AdminRequestId, AdminResponse};

pub(crate) type RequestId = AdminRequestId;

pub(crate) fn response(reply: &crate::commands::CommandReply) -> AdminResponse {
    let view = reply.view();
    AdminResponse {
        success: !matches!(
            view.stage(),
            minimium_commands::results::Stage::Rejected
                | minimium_commands::results::Stage::Partial
        ),
        error_code: view.error_code.map(str::to_owned),
        key: view.error_subject.map(str::to_owned),
        text: crate::command_output::render_reply_view(&view)
            .as_str()
            .to_owned(),
        usage: None,
    }
}

pub(crate) fn parse(
    line: &str,
    catalog: &minimium_commands::operation::OperationCatalog,
) -> Result<crate::server_command::ServerCommand, AdminResponse> {
    crate::server_command::bindings::parse(line, catalog).map_err(|error| AdminResponse {
        success: false,
        error_code: Some(error.code.as_str().to_owned()),
        key: error.subject,
        text: error.reason,
        usage: error.usage,
    })
}
