use super::{CommandActionResult, CommandOutput, FrontendAction, TextCommandResult};
use minimium_commands::operation::CommandValue;
use minimium_commands::results::{
    OperationContext, OperationFailure, OperationReceipt, OperationResult, Payload, Stage,
};
use minimium_commands::{CommandError, CommandErrorCode};
use minimium_localization::{Locale, Localizer, UiMessage};

pub(crate) enum ClientPayload {
    Native(Payload),
    Player(super::player_actions::PlayerReceipt),
    Connection(super::connection_adapter::ConnectionReport),
    #[cfg(feature = "desktop")]
    MeteorDiagnostic(super::meteor_adapter::MeteorDiagnosticReport),
    CatalogHelp(super::help::CatalogHelp),
    Clear,
}
pub(crate) type ClientResult = OperationResult<ClientPayload>;
pub(crate) type Submission = (OperationContext, TextCommandResult);

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

#[cfg(feature = "desktop")]
pub(crate) fn meteor_diagnostic(
    identity: OperationContext,
    stage: Stage,
    report: super::meteor_adapter::MeteorDiagnosticReport,
) -> ClientResult {
    let property = OperationResult::new(
        identity.clone(),
        OperationReceipt::at(stage),
        Ok(Payload::Property(report.property.clone())),
    );
    if let Err(error) = property.validate() {
        return failure(identity, Stage::Rejected, "meteor_diagnostic", error);
    }
    if let Some(snapshot) = &report.snapshot {
        let projection = OperationResult::new(
            identity.clone(),
            OperationReceipt::at(stage),
            Ok(Payload::Value(snapshot.clone())),
        );
        if let Err(error) = projection.validate() {
            return failure(identity, Stage::Rejected, "meteor_diagnostic", error);
        }
    }
    OperationResult::new(
        identity,
        OperationReceipt::at(stage),
        Ok(ClientPayload::MeteorDiagnostic(report)),
    )
}

pub(crate) fn failure(
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
            let console = if matches!(action, FrontendAction::Clear) {
                Some(ClientPayload::Clear)
            } else {
                super::help::CatalogHelp::for_action(&action).map(ClientPayload::CatalogHelp)
            };
            if let Some(payload) = console {
                return (
                    false,
                    Some(OperationResult::new(
                        identity,
                        OperationReceipt::at(Stage::Completed),
                        Ok(payload),
                    )),
                );
            }
            let (exit, result) = handle(identity, action);
            return (exit, Some(result));
        }
        TextCommandResult::Rejected(error) => failure(identity, Stage::Rejected, "input", error),
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
        Ok(ClientPayload::Player(report)) => {
            format!("{id} {stage} {}", report.details(localizer, locale))
        }
        Ok(ClientPayload::Connection(report)) => {
            format!("{id} {stage} {}", report.render(localizer, locale))
        }
        Ok(ClientPayload::Clear) => {
            return CommandActionResult {
                outputs: vec![CommandOutput::Clear],
            }
        }
        Ok(ClientPayload::CatalogHelp(report)) => {
            return CommandActionResult {
                outputs: report
                    .format(localizer, locale)
                    .into_iter()
                    .map(CommandOutput::Line)
                    .collect(),
            }
        }
        Ok(ClientPayload::Native(Payload::Property(value))) => {
            format!("{id} {stage} {}", value.format_with(CommandValue::encode))
        }
        #[cfg(feature = "desktop")]
        Ok(ClientPayload::MeteorDiagnostic(report)) => {
            let mut line = format!(
                "{id} {stage} {}",
                report.property.format_with(CommandValue::encode)
            );
            if let Some(snapshot) = report.snapshot {
                line.push_str(" report=");
                line.push_str(&snapshot.encode());
            }
            line
        }
        Ok(ClientPayload::Native(Payload::Value(CommandValue::Unit))) => format!("{id} {stage}"),
        #[cfg(feature = "console-scripting")]
        Ok(ClientPayload::Native(Payload::Value(value))) if id == "client.scripting.status" => {
            format!(
                "{id} {stage} {}",
                crate::scripting::format_status(&value, locale)
            )
        }
        Ok(ClientPayload::Native(Payload::Value(value))) => {
            format!("{id} {stage} {}", value.encode())
        }
        Ok(ClientPayload::Native(Payload::LegacyText(text))) => format!("{id} {stage} {text}"),
        Err(error) => {
            let header = localizer.format(
                locale,
                &UiMessage::CommandRejected {
                    code: error.code.clone(),
                    subject: error.subject.unwrap_or_else(|| id.clone()),
                },
            );
            let reason = if error.code == CommandErrorCode::InputTooLong.as_str() {
                super::help::input_length_error(error.span, localizer, locale)
            } else {
                error.reason
            };
            let mut text = format!("{stage} {} {header}: {reason}", error.code);
            if let Some(usage) = error.usage {
                text.push('\n');
                text.push_str(&usage)
            }
            text
        }
    };
    let line = match result.receipt.sample_age_ms {
        Some(age) => format!("{line} sample_age_ms={age}"),
        None => line,
    };
    let line = match (&result.identity.session, &result.identity.context) {
        (Some(session), Some(context)) => format!(
            "request={} session={} world={} content_revision={} membership_epoch={} {line}",
            result.identity.request_id,
            session,
            context.instance,
            context.content_revision,
            context.membership_epoch
        ),
        _ => line,
    };
    let line = match result.receipt.settings_base {
        Some(base) => format!("{line} settings_base={base}"),
        None => line,
    };
    let line = if let Some(nonce) = result.identity.correlation_nonce {
        format!(
            "request={} nonce={} {line}",
            result.identity.request_id, nonce
        )
    } else {
        line
    };
    CommandActionResult {
        outputs: line
            .lines()
            .map(|s| CommandOutput::Line(s.into()))
            .collect(),
    }
}
