use minimium_commands::operation::CommandValue;
use minimium_commands::results::OperationResult;
use minimium_commands::results::{OperationFailure, OperationReceipt, Stage};
use minimium_commands::syntax::canonical::AccessKind;
use std::sync::mpsc::SyncSender;

pub(crate) struct HostRequest {
    pub(crate) path: String,
    pub(crate) access: AccessKind,
    pub(crate) arguments: Vec<CommandValue>,
    pub(crate) reply: SyncSender<OperationResult>,
}

pub(crate) fn project(result: crate::client_commands::output::ClientResult) -> OperationResult {
    let identity = result.identity;
    let receipt = result.receipt;
    let outcome = match result.outcome {
        Ok(crate::client_commands::output::ClientPayload::Native(payload)) => Ok(payload),
        Err(failure) => Err(failure),
        Ok(_) => Err(OperationFailure {
            code: "untyped_result".into(),
            subject: Some(identity.operation.0.clone()),
            reason: "operation has no typed script projection".into(),
            usage: None,
            span: None,
            origin: "client_scripting".into(),
        }),
    };
    let receipt = if outcome.is_err()
        && !matches!(
            receipt.stage,
            Stage::Rejected | Stage::Suppressed | Stage::Cancelled | Stage::UnknownAfterDisconnect
        ) {
        OperationReceipt::at(Stage::Rejected)
    } else {
        receipt
    };
    let result = OperationResult::new(identity, receipt, outcome);
    if result.validate().is_ok() {
        result
    } else {
        OperationResult::new(
            result.identity,
            OperationReceipt::at(Stage::Rejected),
            Err(OperationFailure {
                code: "result_limit".into(),
                subject: None,
                reason: "script response exceeds bounded schema".into(),
                usage: None,
                span: None,
                origin: "client_scripting".into(),
            }),
        )
    }
}
