use super::Ledger;
use minimium_commands::operation::CommandValue;
use minimium_commands::results::{OperationContext, OperationResult};
use minimium_commands::results::{OperationFailure, OperationReceipt, Stage};
use minimium_commands::syntax::canonical::AccessKind;
use std::sync::mpsc::SyncSender;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Condvar, Mutex,
};
use std::time::{Duration, Instant};

pub(crate) struct HostRequest {
    pub(crate) run_id: u64,
    pub(crate) path: String,
    pub(crate) access: AccessKind,
    pub(crate) arguments: Vec<CommandValue>,
    pub(crate) camera_condition: Option<CameraCondition>,
    pub(crate) reply: SyncSender<OperationResult>,
    pub(crate) waiter: Arc<Waiter>,
}

#[derive(Clone, Copy)]
pub(crate) struct CameraCondition {
    pub(crate) expected_base: u64,
    pub(crate) expected_revision: u64,
}

pub(crate) struct Waiter {
    result: Mutex<Option<OperationResult>>,
    wake: Condvar,
    ledger: Arc<Mutex<Ledger>>,
    operation_index: usize,
    cancel: Arc<AtomicBool>,
    mutation_pending: Arc<AtomicBool>,
    mutation: bool,
}

impl Waiter {
    fn mark_uncertain(&self, stage: &str, code: &str) {
        if let Ok(mut ledger) = self.ledger.lock() {
            if let Some(call) = ledger.operations.get_mut(self.operation_index) {
                call.stage = stage.into();
                call.error_code = Some(code.into());
            }
        }
    }
    pub(crate) fn record_submission(&self, identity: &OperationContext) {
        if let Ok(mut ledger) = self.ledger.lock() {
            if let Some(call) = ledger.operations.get_mut(self.operation_index) {
                call.session = identity.session;
                call.context = identity.context.clone();
                call.nonce = identity.correlation_nonce;
            }
        }
    }
    pub(crate) fn new(
        ledger: Arc<Mutex<Ledger>>,
        operation_index: usize,
        cancel: Arc<AtomicBool>,
        mutation_pending: Arc<AtomicBool>,
        mutation: bool,
    ) -> Self {
        Self {
            result: Mutex::new(None),
            wake: Condvar::new(),
            ledger,
            operation_index,
            cancel,
            mutation_pending,
            mutation,
        }
    }

    pub(crate) fn complete(&self, result: OperationResult) {
        if self.mutation
            && matches!(
                result.receipt.stage,
                Stage::OwnerCommitted
                    | Stage::Realized
                    | Stage::Durable
                    | Stage::Completed
                    | Stage::Rejected
                    | Stage::Suppressed
            )
        {
            self.mutation_pending.store(false, Ordering::Release);
        }
        if let Ok(mut slot) = self.result.lock() {
            *slot = Some(result.clone());
        }
        if let Ok(mut ledger) = self.ledger.lock() {
            if let Some(call) = ledger.operations.get_mut(self.operation_index) {
                call.stage = result.receipt.stage.label().into();
                call.request_id = Some(result.identity.request_id);
                call.error_code = result
                    .outcome
                    .as_ref()
                    .err()
                    .map(|failure| failure.code.clone());
            }
        }
        self.wake.notify_all();
    }

    pub(crate) fn wait(&self, timeout: Duration) -> Result<OperationResult, &'static str> {
        let _wait_clock = super::metrics::WaitClock::start(&self.ledger);
        let started = Instant::now();
        let mut slot = self.result.lock().map_err(|_| "host_failed")?;
        loop {
            if let Some(result) = slot.as_ref() {
                return Ok(result.clone());
            }
            if self.cancel.load(Ordering::Acquire) {
                self.mark_uncertain("unknown_after_cancel", "cancelled");
                return Err("cancelled");
            }
            let remaining_run = self
                .ledger
                .lock()
                .map_err(|_| "host_failed")?
                .started
                .elapsed();
            if remaining_run >= Duration::from_secs(10) {
                self.mark_uncertain("unknown_after_deadline", "deadline");
                return Err("deadline");
            }
            if started.elapsed() >= timeout {
                self.mark_uncertain("unknown_after_timeout", "wait_timeout");
                return Err("wait_timeout");
            }
            let remaining = timeout
                .saturating_sub(started.elapsed())
                .min(Duration::from_secs(10).saturating_sub(remaining_run))
                .min(Duration::from_millis(50));
            let (next, _) = self
                .wake
                .wait_timeout(slot, remaining)
                .map_err(|_| "host_failed")?;
            slot = next;
        }
    }
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
