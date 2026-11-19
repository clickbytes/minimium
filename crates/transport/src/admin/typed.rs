use super::{queue_reply, Client};
use minimium_commands::admin::{AdminReplyV1, AdminRequestV1, Correlation};
use minimium_commands::results::{OperationFailure, OperationReceipt, OperationResult, Stage};
use minimium_commands::{CommandError, CommandErrorCode};
use std::collections::BTreeMap;
use std::time::Instant;

pub enum AdminInput {
    Legacy(String),
    Typed(AdminRequestV1),
}
impl AdminInput {
    pub fn correlation(&self) -> Option<Correlation> {
        match self {
            Self::Legacy(_) => None,
            Self::Typed(r) => Some(r.correlation),
        }
    }
}

#[derive(Default)]
pub(super) struct Correlations(BTreeMap<u64, u64>);
impl Correlations {
    pub(super) fn admit(&mut self, c: Correlation) -> Result<(), CommandError> {
        if !self.0.contains_key(&c.nonce) && self.0.len() >= 65536 {
            return Err(CommandError::new(
                CommandErrorCode::ResultLimit,
                Some("nonce"),
                "server correlation registry is full",
                None::<String>,
            ));
        }
        let previous = self.0.entry(c.nonce).or_default();
        if c.request_id <= *previous {
            return Err(CommandError::new(
                CommandErrorCode::Conflict,
                Some("request_id"),
                "duplicate or stale request ID",
                None::<String>,
            ));
        }
        *previous = c.request_id;
        Ok(())
    }
}

pub(super) fn reject(client: &mut Client, code: &str, reason: &str, stage: Stage, now: Instant) {
    if let Some((correlation, operation)) = &client.correlation {
        let error = OperationFailure {
            code: code.into(),
            subject: None,
            reason: reason.into(),
            usage: None,
            span: None,
            origin: "admin_admission".into(),
        };
        let result = OperationResult::new(
            correlation.identity(operation.clone()),
            OperationReceipt::at(stage),
            Err(error),
        );
        if send(client, result, now).is_ok() {
            return;
        }
    }
    let msg = super::encode_error_envelope(code, None, reason, None);
    queue_reply(client, format!("{msg}\n").as_bytes(), now)
}

pub(super) fn send(
    client: &mut Client,
    result: OperationResult,
    now: Instant,
) -> Result<(), super::QueueError> {
    let (correlation, _) = client
        .correlation
        .as_ref()
        .ok_or(super::QueueError::Encoding)?;
    let c = *correlation;
    let reply = AdminReplyV1::new(c, result);
    let line = reply
        .encode(c)
        .or_else(|error| {
            let result = c.failure(
                reply.result.identity.operation.clone(),
                Stage::Rejected,
                "admin_response",
                error,
            );
            AdminReplyV1::new(c, result).encode(c)
        })
        .map_err(|_| super::QueueError::Encoding)?;
    queue_reply(client, format!("{line}\n").as_bytes(), now);
    Ok(())
}
