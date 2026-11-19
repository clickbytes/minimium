//! Opt-in administrative protocol, independent of gameplay and save versions.
use crate::operation::{CommandValue, OperationId};
use crate::results::{
    OperationContext, OperationFailure, OperationReceipt, OperationResult, Stage, WorldBinding,
    MAX_ADMIN_BYTES, MAX_PAYLOAD_BYTES,
};
use crate::syntax::canonical::AccessKind;
use crate::{CommandError, CommandErrorCode as Code};
use serde::{Deserialize, Serialize};
pub mod metadata;
pub(crate) mod number;
pub const PREFIX: &str = "@console/1 ";
pub const MAX_REQUEST_BYTES: usize = 512;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Correlation {
    #[serde(with = "number")]
    pub nonce: u64,
    #[serde(with = "number")]
    pub request_id: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminRequestV1 {
    pub schema_version: u16,
    pub correlation: Correlation,
    pub expected_context: Option<WorldBinding>,
    pub payload: AdminPayload,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AdminPayload {
    Text {
        text: String,
    },
    Operation {
        operation_id: OperationId,
        access: AccessKind,
        arguments: Vec<CommandValue>,
        value: Option<CommandValue>,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminReplyV1 {
    pub schema_version: u16,
    #[serde(with = "number")]
    pub nonce: u64,
    pub result: OperationResult,
}

fn error(code: Code, reason: impl Into<String>) -> CommandError {
    CommandError::new(code, Some("admin"), reason, None::<String>)
}
fn data_error(e: serde_json::Error) -> CommandError {
    error(Code::InvalidArguments, e.to_string())
}
fn body(line: &str, limit: usize) -> Result<&str, CommandError> {
    if line.len() > limit || line.chars().any(char::is_control) {
        return Err(error(
            Code::ResultLimit,
            "invalid administrative frame size or control character",
        ));
    }
    line.strip_prefix(PREFIX).ok_or_else(|| {
        error(
            Code::InvalidArguments,
            "unsupported administrative protocol version",
        )
    })
}
impl Correlation {
    pub fn identity(self, operation: OperationId) -> OperationContext {
        OperationContext {
            request_id: self.request_id,
            correlation_nonce: Some(self.nonce),
            operation,
            scope: "Server".into(),
            session: None,
            context: None,
        }
    }
    pub fn failure(
        self,
        operation: OperationId,
        stage: Stage,
        origin: &str,
        e: CommandError,
    ) -> OperationResult {
        OperationResult::new(
            self.identity(operation),
            OperationReceipt::at(stage),
            Err(OperationFailure::command(origin, e)),
        )
    }
}
impl AdminRequestV1 {
    pub fn new(correlation: Correlation, payload: AdminPayload) -> Self {
        Self {
            schema_version: 1,
            correlation,
            expected_context: None,
            payload,
        }
    }
    pub fn operation(&self) -> OperationId {
        match &self.payload {
            AdminPayload::Operation { operation_id, .. } => operation_id.clone(),
            _ => OperationId("ui.console.input".into()),
        }
    }
    pub fn validate(&self) -> Result<(), CommandError> {
        if self.schema_version != 1
            || self.correlation.nonce == 0
            || self.correlation.request_id == 0
        {
            return Err(error(
                Code::InvalidArguments,
                "invalid admin schema or correlation",
            ));
        }
        if let AdminPayload::Operation {
            arguments, value, ..
        } = &self.payload
        {
            if arguments.len() > 32
                || arguments
                    .iter()
                    .chain(value)
                    .any(|a| matches!(a, CommandValue::Page { .. } | CommandValue::Record(_)))
            {
                return Err(error(
                    Code::InvalidArguments,
                    "expected at most 32 scalar arguments",
                ));
            }
        }
        match &self.payload {
            AdminPayload::Text { text }
                if text.trim().is_empty() || text.chars().any(char::is_control) =>
            {
                Err(error(Code::InvalidArguments, "invalid command text"))
            }
            AdminPayload::Operation {
                operation_id,
                access,
                arguments,
                value,
            } if operation_id.0.is_empty()
                || operation_id.0.len() > 128
                || (matches!(access, AccessKind::Write) != value.is_some())
                || (*access != AccessKind::Invoke && !arguments.is_empty()) =>
            {
                Err(error(
                    Code::InvalidArguments,
                    "invalid operation arguments or assignment",
                ))
            }
            _ => Ok(()),
        }
    }
    pub fn decode(line: &str) -> Result<Self, CommandError> {
        let req: Self = serde_json::from_str(body(line, MAX_REQUEST_BYTES)?).map_err(data_error)?;
        req.validate()?;
        Ok(req)
    }
    pub fn encode(&self) -> Result<String, CommandError> {
        self.validate()?;
        frame(self, MAX_REQUEST_BYTES)
    }
}

fn frame(value: &impl Serialize, max: usize) -> Result<String, CommandError> {
    let mut bytes = vec![0; max];
    bytes[..PREFIX.len()].copy_from_slice(PREFIX.as_bytes());
    let mut remaining = &mut bytes[PREFIX.len()..];
    serde_json::to_writer(&mut remaining, value)
        .map_err(|_| error(Code::ResultLimit, "encoded admin frame exceeds byte budget"))?;
    let size = max - remaining.len();
    bytes.truncate(size);
    Ok(String::from_utf8(bytes).expect("JSON emits UTF-8"))
}
impl AdminReplyV1 {
    pub fn new(correlation: Correlation, result: OperationResult) -> Self {
        Self {
            schema_version: 1,
            nonce: correlation.nonce,
            result,
        }
    }
    fn validate(&self, expected: Correlation) -> Result<(), CommandError> {
        if self.schema_version != 1
            || self.nonce != expected.nonce
            || self.result.identity.correlation_nonce != Some(expected.nonce)
            || expected.request_id != self.result.identity.request_id
            || expected.request_id == 0
            || expected.nonce == 0
        {
            return Err(error(
                Code::Conflict,
                "administrative reply version or correlation mismatch",
            ));
        }
        self.result.validate()?;
        frame(
            &(&self.result.outcome, &self.result.receipt.effects),
            MAX_PAYLOAD_BYTES + PREFIX.len(),
        )?;
        Ok(())
    }
    pub fn encode(&self, expected: Correlation) -> Result<String, CommandError> {
        self.validate(expected)?;
        frame(self, MAX_ADMIN_BYTES - 1)
    }
    pub fn decode(line: &str, expected: Correlation) -> Result<Self, CommandError> {
        let reply: Self =
            serde_json::from_str(body(line, MAX_ADMIN_BYTES - 1)?).map_err(data_error)?;
        reply.validate(expected)?;
        Ok(reply)
    }
}
