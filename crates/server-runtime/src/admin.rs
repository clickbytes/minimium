use minimium_transport::{AdminRequestId, AdminResponse};

pub(crate) type RequestId = AdminRequestId;

pub(crate) fn response(reply: &crate::commands::CommandReply) -> AdminResponse {
    AdminResponse {
        success: reply.is_success(),
        error_code: reply.error_code().map(str::to_owned),
        key: None,
        text: crate::command_output::render_reply(reply)
            .as_str()
            .to_owned(),
        usage: None,
    }
}

pub(crate) fn parse(line: &str) -> Result<crate::server_command::ServerCommand, AdminResponse> {
    crate::server_command::ServerCommand::parse(line).map_err(|error| AdminResponse {
        success: false,
        error_code: Some(error.code.as_str().to_owned()),
        key: error.subject,
        text: error.reason,
        usage: error.usage,
    })
}
