//! Opt-in client host for bounded local Rhai jobs.
mod bindings;
mod bridge;
mod errors;
mod limits;
mod registration;
mod values;

use crate::client_commands::ScriptControl;
use minimium_commands::operation::CommandValue;
use minimium_commands::results::{
    OperationContext, OperationReceipt, OperationResult, Payload, Stage,
};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, Receiver},
    Arc, Mutex,
};
use std::time::Instant;

pub(crate) use bridge::project;
pub(crate) use bridge::HostRequest;
pub(crate) use errors::format_status;

#[derive(Default)]
pub(crate) struct ScriptingHost {
    next_id: u64,
    runs: BTreeMap<u64, Run>,
    pending: Option<HostRequest>,
    receipts: Vec<PendingReceipt>,
}

struct PendingReceipt {
    run_id: u64,
    identity: OperationContext,
    waiter: Arc<bridge::Waiter>,
    disconnected: bool,
}

struct Run {
    ledger: Arc<Mutex<Ledger>>,
    cancel: Arc<AtomicBool>,
    calls: Receiver<HostRequest>,
}

pub(crate) struct Ledger {
    pub(crate) started: Instant,
    pub(crate) phase: &'static str,
    pub(crate) calls: u32,
    pub(crate) mutations: u32,
    pub(crate) output: String,
    pub(crate) result: Option<String>,
    pub(crate) error: Option<String>,
    pub(crate) operations: Vec<CallRecord>,
}

pub(crate) struct CallRecord {
    pub(crate) path: String,
    pub(crate) stage: String,
    pub(crate) request_id: Option<u64>,
    pub(crate) error_code: Option<String>,
    pub(crate) session: Option<u64>,
    pub(crate) context: Option<minimium_commands::results::WorldBinding>,
    pub(crate) nonce: Option<u64>,
}

impl ScriptingHost {
    pub(crate) fn register_submission(&mut self, request: &HostRequest, result: &OperationResult) {
        request.waiter.record_submission(&result.identity);
        if !self.runs.contains_key(&request.run_id) {
            request
                .waiter
                .complete(unknown_after_disconnect(&result.identity));
            return;
        }
        if result.receipt.stage == Stage::Submitted && result.outcome.is_ok() {
            if self.receipts.len() >= 16 {
                request
                    .waiter
                    .complete(unknown_after_disconnect(&result.identity));
                return;
            }
            self.receipts.push(PendingReceipt {
                run_id: request.run_id,
                identity: result.identity.clone(),
                waiter: Arc::clone(&request.waiter),
                disconnected: false,
            });
        } else {
            request.waiter.complete(result.clone());
        }
    }

    pub(crate) fn record_owner_result(&mut self, result: OperationResult) {
        let Some(index) = self.receipts.iter().position(|pending| {
            pending.identity.request_id == result.identity.request_id
                && pending.identity.correlation_nonce == result.identity.correlation_nonce
                && pending.identity.operation == result.identity.operation
        }) else {
            return;
        };
        let pending = self.receipts.remove(index);
        pending.waiter.complete(result);
    }

    pub(crate) fn observe_session(&mut self, session: Option<u64>) {
        for pending in &mut self.receipts {
            if !pending.disconnected && pending.identity.session != session {
                pending
                    .waiter
                    .complete(unknown_after_disconnect(&pending.identity));
                pending.disconnected = true;
            }
        }
    }
    pub(crate) fn operate(
        &mut self,
        identity: OperationContext,
        action: ScriptControl,
    ) -> OperationResult {
        let operation = identity.operation.0.clone();
        let stage = if matches!(action, ScriptControl::Status(_)) {
            Stage::Completed
        } else {
            Stage::Submitted
        };
        let result = match action {
            ScriptControl::RunFile(path) => self.start(path),
            ScriptControl::Status(id) => self.status(id),
            ScriptControl::Cancel(id) => self.cancel(id),
        };
        match result {
            Ok(value) => OperationResult::new(
                identity,
                OperationReceipt::at(stage),
                Ok(Payload::Value(value)),
            ),
            Err(error) => OperationResult::new(
                identity,
                OperationReceipt::at(Stage::Rejected),
                Err(minimium_commands::results::OperationFailure {
                    code: error.code.into(),
                    subject: Some(operation),
                    reason: error.reason,
                    usage: None,
                    span: None,
                    origin: "client_scripting".into(),
                }),
            ),
        }
    }

    fn start(&mut self, path: String) -> Result<CommandValue, errors::ScriptError> {
        if self.runs.values().any(|run| {
            run.ledger
                .lock()
                .is_ok_and(|ledger| ledger.phase == "running")
        }) {
            return Err(errors::ScriptError::new(
                "busy",
                "one script is already active",
            ));
        }
        if path.is_empty() || path.len() > 4096 || path.contains('\0') {
            return Err(errors::ScriptError::new(
                "invalid_path",
                "expected a local path",
            ));
        }
        if self.runs.len() >= 16 {
            let retired = self.runs.iter().find_map(|(&id, run)| {
                let active = run
                    .ledger
                    .lock()
                    .is_ok_and(|ledger| ledger.phase == "running");
                (!active && !self.receipts.iter().any(|pending| pending.run_id == id)).then_some(id)
            });
            let id = retired.ok_or_else(|| {
                errors::ScriptError::new(
                    "ledger_full",
                    "script run ledger has unresolved submissions",
                )
            })?;
            self.runs.remove(&id);
        }
        let cwd = std::env::current_dir()
            .map_err(|e| errors::ScriptError::new("host_io", e.to_string()))?;
        let id = self.next_id.checked_add(1).ok_or_else(|| {
            errors::ScriptError::new("id_exhausted", "script run identifiers exhausted")
        })?;
        let ledger = Arc::new(Mutex::new(Ledger {
            started: Instant::now(),
            phase: "running",
            calls: 0,
            mutations: 0,
            output: String::new(),
            result: None,
            error: None,
            operations: Vec::new(),
        }));
        let cancel = Arc::new(AtomicBool::new(false));
        let (sender, calls) = mpsc::sync_channel(1);
        let thread_ledger = Arc::clone(&ledger);
        let thread_cancel = Arc::clone(&cancel);
        std::thread::Builder::new()
            .name(format!("script-{id}"))
            .spawn(move || {
                let path = cwd.join(PathBuf::from(path));
                bindings::run(id, path, sender, thread_cancel, thread_ledger);
            })
            .map_err(|e| errors::ScriptError::new("worker_unavailable", e.to_string()))?;
        self.next_id = id;
        self.runs.insert(
            id,
            Run {
                ledger,
                cancel,
                calls,
            },
        );
        Ok(CommandValue::Record(BTreeMap::from([
            ("run_id".into(), CommandValue::U64(id)),
            ("phase".into(), CommandValue::Enum("running".into())),
        ])))
    }

    fn status(&self, id: u64) -> Result<CommandValue, errors::ScriptError> {
        let run = self.runs.get(&id).ok_or_else(|| {
            errors::ScriptError::new("unknown_run", "script run was not retained")
        })?;
        let ledger = run
            .ledger
            .lock()
            .map_err(|_| errors::ScriptError::new("worker_failed", "script ledger unavailable"))?;
        let mut record = BTreeMap::from([
            ("run_id".into(), CommandValue::U64(id)),
            ("phase".into(), CommandValue::Enum(ledger.phase.into())),
            (
                "elapsed_ms".into(),
                CommandValue::U64(
                    ledger
                        .started
                        .elapsed()
                        .as_millis()
                        .min(u128::from(u64::MAX)) as u64,
                ),
            ),
            ("calls".into(), CommandValue::U64(u64::from(ledger.calls))),
            (
                "mutations".into(),
                CommandValue::U64(u64::from(ledger.mutations)),
            ),
            (
                "output_bytes".into(),
                CommandValue::U64(ledger.output.len() as u64),
            ),
            (
                "output_preview".into(),
                CommandValue::Text(ledger.output.chars().take(4096).collect()),
            ),
            (
                "operations".into(),
                CommandValue::Page {
                    rows: ledger
                        .operations
                        .iter()
                        .map(|call| {
                            CommandValue::Record(BTreeMap::from([
                                ("path".into(), CommandValue::Text(call.path.clone())),
                                ("stage".into(), CommandValue::Enum(call.stage.clone())),
                                (
                                    "request_id".into(),
                                    call.request_id
                                        .map_or(CommandValue::Unit, CommandValue::U64),
                                ),
                                (
                                    "error_code".into(),
                                    call.error_code.as_ref().map_or(CommandValue::Unit, |code| {
                                        CommandValue::Enum(code.clone())
                                    }),
                                ),
                                (
                                    "session".into(),
                                    call.session.map_or(CommandValue::Unit, CommandValue::U64),
                                ),
                                (
                                    "nonce".into(),
                                    call.nonce.map_or(CommandValue::Unit, CommandValue::U64),
                                ),
                                (
                                    "context".into(),
                                    call.context.as_ref().map_or(CommandValue::Unit, |context| {
                                        CommandValue::Record(BTreeMap::from([
                                            (
                                                "instance".into(),
                                                CommandValue::U128(context.instance),
                                            ),
                                            (
                                                "content_revision".into(),
                                                CommandValue::U64(context.content_revision),
                                            ),
                                            (
                                                "membership_epoch".into(),
                                                CommandValue::U64(context.membership_epoch),
                                            ),
                                        ]))
                                    }),
                                ),
                            ]))
                        })
                        .collect(),
                    next_cursor: None,
                },
            ),
        ]);
        if let Some(value) = &ledger.result {
            record.insert("result_bytes".into(), CommandValue::U64(value.len() as u64));
            record.insert(
                "result_preview".into(),
                CommandValue::Text(value.chars().take(4096).collect()),
            );
        }
        if let Some(value) = &ledger.error {
            record.insert(
                "error".into(),
                CommandValue::Text(value.chars().take(4096).collect()),
            );
        }
        Ok(CommandValue::Record(record))
    }

    fn cancel(&self, id: u64) -> Result<CommandValue, errors::ScriptError> {
        let run = self.runs.get(&id).ok_or_else(|| {
            errors::ScriptError::new("unknown_run", "script run was not retained")
        })?;
        run.cancel.store(true, Ordering::Release);
        Ok(CommandValue::Record(BTreeMap::from([
            ("run_id".into(), CommandValue::U64(id)),
            ("cancel_requested".into(), CommandValue::Bool(true)),
        ])))
    }

    pub(crate) fn pump(&mut self) {
        if self.pending.is_some() {
            return;
        }
        for run in self.runs.values() {
            if let Ok(request) = run.calls.try_recv() {
                self.pending = Some(request);
                break;
            }
        }
    }

    pub(crate) fn take_request(&mut self) -> Option<HostRequest> {
        self.pending.take()
    }
}

fn unknown_after_disconnect(identity: &OperationContext) -> OperationResult {
    OperationResult::new(
        identity.clone(),
        OperationReceipt::at(Stage::UnknownAfterDisconnect),
        Err(minimium_commands::results::OperationFailure {
            code: "unknown_after_disconnect".into(),
            subject: Some(identity.operation.0.clone()),
            reason: "session generation changed after submission; owner outcome is uncertain"
                .into(),
            usage: None,
            span: None,
            origin: "client_scripting".into(),
        }),
    )
}
