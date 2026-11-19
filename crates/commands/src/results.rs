//! Versioned, bounded operation outcomes shared by text and machine consumers.
use crate::operation::{CommandValue, OperationId};
use crate::{CommandError, CommandErrorCode, PropertyValue};
use serde::{Deserialize, Serialize};

mod limits;
pub mod property;
pub use limits::{MAX_ADMIN_BYTES, MAX_PAYLOAD_BYTES};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
    LegacyReported,
    Rejected,
    Suppressed,
    Submitted,
    OwnerAccepted,
    OwnerCommitted,
    Realized,
    Partial,
    Completed,
    Durable,
    Cancelled,
    UnknownAfterDisconnect,
}

impl Stage {
    pub const fn label(self) -> &'static str {
        match self {
            Self::LegacyReported => "legacy_reported",
            Self::Rejected => "rejected",
            Self::Suppressed => "suppressed",
            Self::Submitted => "submitted",
            Self::OwnerAccepted => "owner_accepted",
            Self::OwnerCommitted => "owner_committed",
            Self::Realized => "realized",
            Self::Partial => "partial",
            Self::Completed => "completed",
            Self::Durable => "durable",
            Self::Cancelled => "cancelled",
            Self::UnknownAfterDisconnect => "unknown_after_disconnect",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationReceipt {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::admin::number::optional"
    )]
    pub sample_age_ms: Option<u64>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "crate::admin::number::optional"
    )]
    pub settings_base: Option<u64>,
    pub stage: Stage,
    #[serde(with = "crate::admin::number::optional")]
    pub revision: Option<u64>,
    pub freshness: Option<property::Freshness>,
    pub effects: Vec<CommandValue>,
    pub should_stop: bool,
}

impl OperationReceipt {
    pub fn at(stage: Stage) -> Self {
        Self {
            stage,
            settings_base: None,
            sample_age_ms: None,
            revision: None,
            freshness: None,
            effects: Vec::new(),
            should_stop: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorldBinding {
    #[serde(with = "crate::admin::number")]
    pub instance: u128,
    #[serde(with = "crate::admin::number")]
    pub content_revision: u64,
    #[serde(with = "crate::admin::number")]
    pub membership_epoch: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationContext {
    #[serde(default, with = "crate::admin::number::optional")]
    pub correlation_nonce: Option<u64>,
    #[serde(with = "crate::admin::number")]
    pub request_id: u64,
    pub operation: OperationId,
    pub scope: String,
    #[serde(with = "crate::admin::number::optional")]
    pub session: Option<u64>,
    pub context: Option<WorldBinding>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Payload {
    Property(PropertyValue<CommandValue>),
    Value(CommandValue),
    /// Display-only output from a handler that has no machine payload.
    LegacyText(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationFailure {
    pub code: String,
    pub subject: Option<String>,
    pub reason: String,
    pub usage: Option<String>,
    pub span: Option<crate::SourceSpan>,
    pub origin: String,
}

impl OperationFailure {
    pub fn command(origin: &str, error: CommandError) -> Self {
        Self {
            code: error.code.as_str().into(),
            subject: error.subject,
            reason: error.reason,
            usage: error.usage,
            span: error.span,
            origin: origin.into(),
        }
    }
}

/// Host projections may borrow an existing exhaustive payload. Wire projections own their data.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationResult<T = Payload> {
    pub schema_version: u16,
    pub identity: OperationContext,
    pub receipt: OperationReceipt,
    pub outcome: Result<T, OperationFailure>,
}

impl<T> OperationResult<T> {
    pub fn new(
        identity: OperationContext,
        receipt: OperationReceipt,
        outcome: Result<T, OperationFailure>,
    ) -> Self {
        Self {
            schema_version: 1,
            identity,
            receipt,
            outcome,
        }
    }
}

impl OperationResult {
    pub fn validate(&self) -> Result<(), CommandError> {
        let needs_error = matches!(
            self.receipt.stage,
            Stage::Rejected | Stage::Suppressed | Stage::Cancelled | Stage::UnknownAfterDisconnect
        );
        if self.outcome.is_err() != needs_error
            || self.identity.request_id == 0
            || self.identity.operation.0.is_empty()
        {
            return Err(CommandError::new(
                CommandErrorCode::InvalidValue,
                Some("result"),
                "inconsistent operation result",
                None::<String>,
            ));
        }
        if self.schema_version != 1 || !limits::fits(self) {
            return Err(CommandError::new(
                CommandErrorCode::ResultLimit,
                Some(&self.identity.operation.0),
                "operation result exceeds schema or resource limits",
                None::<String>,
            ));
        }
        Ok(())
    }
}
