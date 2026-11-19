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
pub struct OperationReceipt {
    pub stage: Stage,
    pub revision: Option<u64>,
    pub freshness: Option<property::Freshness>,
    pub effects: Vec<CommandValue>,
    pub should_stop: bool,
}

impl OperationReceipt {
    pub fn at(stage: Stage) -> Self {
        Self {
            stage,
            revision: None,
            freshness: None,
            effects: Vec::new(),
            should_stop: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldBinding {
    pub instance: u128,
    pub content_revision: u64,
    pub membership_epoch: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OperationContext {
    pub request_id: u64,
    pub operation: OperationId,
    pub scope: String,
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
