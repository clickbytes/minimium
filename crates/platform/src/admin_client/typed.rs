use super::{exchange, ServerEndpoint, CONNECT_TIMEOUT, RESPONSE_TIMEOUT};
use minimium_commands::admin::{AdminPayload, AdminReplyV1, AdminRequestV1, Correlation};
use minimium_commands::results::Stage;
use minimium_commands::{CommandError, CommandErrorCode as Code};
use std::{io, time::Instant};

pub fn new_nonce() -> io::Result<u64> {
    loop {
        let n = u64::from_le_bytes(crate::endpoint::new_random_bytes()?);
        if n != 0 {
            return Ok(n);
        }
    }
}

/// A failure after delivery cannot confirm or cancel a mutation.
pub fn request_v1(endpoint: &ServerEndpoint, request: &AdminRequestV1) -> AdminReplyV1 {
    request_v1_cancellable(endpoint, request, &Delivery::default())
}

#[derive(Default)]
pub struct Delivery(std::sync::atomic::AtomicU8);
impl Delivery {
    pub fn cancel_before_send(&self) -> bool {
        self.0
            .compare_exchange(
                0,
                2,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .is_ok()
    }
    pub(super) fn begin(&self) -> bool {
        self.0
            .compare_exchange(
                0,
                1,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .is_ok()
    }
}
pub fn request_v1_cancellable(
    endpoint: &ServerEndpoint,
    request: &AdminRequestV1,
    delivery: &Delivery,
) -> AdminReplyV1 {
    let c = request.correlation;
    let failure = |stage, code, reason: String| {
        AdminReplyV1::new(
            c,
            c.failure(
                request.operation(),
                stage,
                "admin_client",
                CommandError::new(code, Some("admin"), reason, None::<String>),
            ),
        )
    };
    let line = match request.encode() {
        Ok(line) => line,
        Err(e) => {
            return AdminReplyV1::new(
                c,
                c.failure(request.operation(), Stage::Rejected, "admin_encoder", e),
            )
        }
    };
    let mut sent = false;
    let response = match exchange(
        endpoint,
        &line,
        Instant::now() + CONNECT_TIMEOUT + RESPONSE_TIMEOUT,
        &mut sent,
        Some(delivery),
    ) {
        Ok(text) => text,
        Err(e) => {
            return if sent {
                failure(
                    Stage::UnknownAfterDisconnect,
                    Code::UnknownAfterDisconnect,
                    e.to_string(),
                )
            } else {
                failure(Stage::Cancelled, Code::Unavailable, e.to_string())
            }
        }
    };
    let line = response.strip_suffix('\n').unwrap_or(&response);
    if line == "ERR unauthorized" {
        return failure(
            Stage::Rejected,
            Code::PermissionDenied,
            "administrative authentication failed".into(),
        );
    }
    match AdminReplyV1::decode(line, c) {
        Ok(reply) if matches!(&request.payload,AdminPayload::Operation{operation_id,..} if operation_id!=&reply.result.identity.operation) => {
            failure(
                Stage::UnknownAfterDisconnect,
                Code::Conflict,
                "reply operation does not match request".into(),
            )
        }
        Ok(reply) => reply,
        Err(e) => failure(Stage::UnknownAfterDisconnect, e.code, e.to_string()),
    }
}

pub fn text_request(text: String) -> io::Result<AdminRequestV1> {
    Ok(AdminRequestV1::new(
        Correlation {
            nonce: new_nonce()?,
            request_id: 1,
        },
        AdminPayload::Text { text },
    ))
}
