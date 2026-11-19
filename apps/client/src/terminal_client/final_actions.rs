//! Remaining terminal actions after shared owner and capability handlers.
use crate::application::ClientApplication;
use crate::client_commands::{output as dispatch, CommandSource, FrontendAction};
use minimium_commands::results::{
    OperationContext, OperationReceipt, OperationResult, Payload, Stage,
};

pub(super) fn handle(
    application: &mut ClientApplication,
    identity: OperationContext,
    action: FrontendAction,
) -> dispatch::ClientResult {
    if let Some(result) = application.admin_operation(&identity, &action, CommandSource::Terminal) {
        return result;
    }
    if let FrontendAction::Print(text) = action {
        return dispatch::native(OperationResult::new(
            identity,
            OperationReceipt::at(Stage::Completed),
            Ok(Payload::Value(
                minimium_commands::operation::CommandValue::Text(text),
            )),
        ));
    }
    dispatch::failure(
        identity,
        Stage::Rejected,
        "client_binding",
        minimium_commands::CommandError::new(
            minimium_commands::CommandErrorCode::Unavailable,
            Some("client operation"),
            "catalog action has no terminal binding",
            None::<String>,
        ),
    )
}
