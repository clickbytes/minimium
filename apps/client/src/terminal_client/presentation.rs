use crate::client_commands::{self, output as dispatch, FrontendAction};
use minimium_commands::results::{OperationContext, Stage};
use minimium_commands::{CommandError, CommandErrorCode};

pub(super) fn unavailable(
    identity: &OperationContext,
    action: &FrontendAction,
) -> Option<dispatch::ClientResult> {
    let reason = if client_commands::meteor_adapter::MeteorOperation::from_action(action).is_some()
    {
        "this host has no local meteor presentation"
    } else if client_commands::bolid_preview::BolidPreviewOperation::from_action(action).is_some() {
        "this host has no local bolid preview"
    } else if client_commands::render_queries::RenderQuery::from_action(action).is_some() {
        "renderer_missing: this host has no renderer or prepared presentation sample"
    } else {
        return None;
    };
    Some(dispatch::failure(
        identity.clone(),
        Stage::Rejected,
        "TextHost",
        CommandError::new(
            CommandErrorCode::Unavailable,
            Some("Renderer"),
            reason,
            None::<String>,
        ),
    ))
}
