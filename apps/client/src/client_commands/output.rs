use super::{CommandActionResult, CommandOutput, FrontendAction, TextCommandResult};
use crate::application::SuppressionReason;
use minimium_commands::operation::CommandValue;
use minimium_commands::results::{
    OperationContext, OperationFailure, OperationReceipt, OperationResult, Payload, Stage,
};
use minimium_commands::{CommandError, CommandErrorCode};
use minimium_localization::{Locale, Localizer, UiMessage};

pub(crate) enum ClientPayload {
    Native(Payload),
    // Preserve localization tokens, clear instructions and capability reports until display.
    LegacyText(CommandActionResult),
}
pub(crate) type ClientResult = OperationResult<ClientPayload>;
pub(crate) type Submission = (OperationContext, TextCommandResult);

pub(crate) fn legacy(
    context: OperationContext,
    output: CommandActionResult,
    localizer: &Localizer,
    locale: Locale,
) -> ClientResult {
    // Retain the original localized UI tokens. Bound their emitted representation
    // using the same codec as other results before handing them to the frontend.
    let mut text = String::new();
    for item in &output.outputs {
        let line = match item {
            CommandOutput::Line(s) => s.clone(),
            CommandOutput::ClientText { message, .. } => message.localized(localizer, locale),
            #[cfg(feature = "desktop")]
            CommandOutput::UiMessage(m) | CommandOutput::UiError(m) => localizer.format(locale, m),
            #[cfg(feature = "desktop")]
            CommandOutput::Presentation(super::PresentationResult::Output(lines)) => {
                lines.join("\n")
            }
            CommandOutput::Presentation(super::PresentationResult::Unavailable(reason)) => {
                reason.message()
            }
            CommandOutput::Clear => String::new(),
        };
        if !text.is_empty() {
            text.push('\n');
        }
        if text.len().saturating_add(line.len()) > minimium_commands::results::MAX_PAYLOAD_BYTES {
            return failure(
                context,
                Stage::Rejected,
                "output",
                CommandError::new(
                    CommandErrorCode::ResultLimit,
                    Some("legacy_text"),
                    "legacy output exceeds payload limit",
                    None::<String>,
                ),
            );
        }
        text.push_str(&line);
    }
    let report = OperationResult::new(
        context,
        OperationReceipt::at(Stage::LegacyReported),
        Ok(Payload::LegacyText(text)),
    );
    if let Err(error) = report.validate() {
        return failure(report.identity, Stage::Rejected, "output", error);
    }
    OperationResult::new(
        report.identity,
        report.receipt,
        Ok(ClientPayload::LegacyText(output)),
    )
}

pub(crate) fn native(result: OperationResult) -> ClientResult {
    if let Err(error) = result.validate() {
        return failure(result.identity, Stage::Rejected, "result", error);
    }
    OperationResult::new(
        result.identity,
        result.receipt,
        result.outcome.map(ClientPayload::Native),
    )
}

fn failure(
    identity: OperationContext,
    stage: Stage,
    origin: &str,
    error: CommandError,
) -> ClientResult {
    OperationResult::new(
        identity,
        OperationReceipt::at(stage),
        Err(OperationFailure::command(origin, error)),
    )
}

pub(crate) fn dispatch(
    (identity, outcome): Submission,
    handle: impl FnOnce(OperationContext, FrontendAction) -> (bool, ClientResult),
) -> (bool, Option<ClientResult>) {
    let result = match outcome {
        TextCommandResult::Empty => return (false, None),
        TextCommandResult::Action(action) => {
            let (exit, result) = handle(identity, action);
            return (exit, Some(result));
        }
        TextCommandResult::Rejected(error) => failure(identity, Stage::Rejected, "input", error),
        TextCommandResult::Queued { .. } => native(OperationResult::new(
            identity,
            OperationReceipt::at(Stage::Submitted),
            Ok(Payload::Value(CommandValue::Unit)),
        )),
        TextCommandResult::QueueRejected { error, .. } => {
            let code = match error {
                crate::session::QueueError::StaleWorldContext => CommandErrorCode::StaleContext,
                _ => CommandErrorCode::Unavailable,
            };
            let error = CommandError::new(
                code,
                Some(&identity.operation.0),
                error.to_string(),
                None::<String>,
            );
            failure(identity, Stage::Rejected, "client_queue", error)
        }
        TextCommandResult::Suppressed(reason) => {
            let reason = match reason {
                SuppressionReason::Paused => "paused",
                SuppressionReason::NoSession => "no session",
                SuppressionReason::SessionNotConnected => "session not connected",
                SuppressionReason::ToolUnavailable => "tool unavailable",
                SuppressionReason::ToolPendingOrCoolingDown => "tool pending or cooling down",
                SuppressionReason::WeaponInputCooldown => "weapon input cooldown",
                SuppressionReason::MapTeleportPending => "map teleport pending",
            };
            let error = CommandError::new(
                CommandErrorCode::Unavailable,
                Some(&identity.operation.0),
                reason,
                None::<String>,
            );
            failure(identity, Stage::Suppressed, "client", error)
        }
    };
    (false, Some(result))
}

pub(crate) fn display(
    result: Option<ClientResult>,
    localizer: &Localizer,
    locale: Locale,
) -> CommandActionResult {
    let Some(result) = result else {
        return CommandActionResult::default();
    };
    let stage = result.receipt.stage.label();
    let id = &result.identity.operation.0;
    let line = match result.outcome {
        Ok(ClientPayload::LegacyText(output)) => return output,
        Ok(ClientPayload::Native(Payload::Property(value))) => {
            format!("{id} {stage} {}", value.format_with(CommandValue::encode))
        }
        Ok(ClientPayload::Native(Payload::Value(CommandValue::Unit))) => format!("{id} {stage}"),
        Ok(ClientPayload::Native(Payload::Value(value))) => {
            format!("{id} {stage} {}", value.encode())
        }
        Ok(ClientPayload::Native(Payload::LegacyText(text))) => text,
        Err(error) => {
            let header = localizer.format(
                locale,
                &UiMessage::CommandRejected {
                    code: error.code.clone(),
                    subject: error.subject.unwrap_or_else(|| id.clone()),
                },
            );
            let mut text = format!("{stage} {} {header}: {}", error.code, error.reason);
            if let Some(usage) = error.usage {
                text.push('\n');
                text.push_str(&usage)
            }
            text
        }
    };
    CommandActionResult {
        outputs: line
            .lines()
            .map(|s| CommandOutput::Line(s.into()))
            .collect(),
    }
}
