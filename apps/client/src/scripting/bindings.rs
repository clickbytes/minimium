use super::{
    bridge::{CameraCondition, HostRequest, Waiter},
    limits, values, CallRecord, Ledger,
};
use minimium_commands::operation::Effect;
use minimium_commands::syntax::canonical::AccessKind;
use rhai::{Dynamic, Engine, EvalAltResult, OptimizationLevel, Scope};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, SyncSender, TrySendError},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

#[derive(Clone)]
pub(super) struct Bridge {
    run_id: u64,
    calls: SyncSender<HostRequest>,
    cancel: Arc<AtomicBool>,
    ledger: Arc<Mutex<Ledger>>,
    mutation_pending: Arc<AtomicBool>,
}

pub(super) struct CallOutcome {
    pub(super) result: minimium_commands::results::OperationResult,
    pub(super) waiter: Arc<Waiter>,
}

impl Bridge {
    fn update_last(&self, stage: &str, request_id: Option<u64>, error_code: Option<&str>) {
        if let Ok(mut ledger) = self.ledger.lock() {
            if let Some(last) = ledger.operations.last_mut() {
                last.stage = stage.into();
                last.request_id = request_id;
                last.error_code = error_code.map(str::to_owned);
            }
        }
    }

    pub(super) fn call(
        &self,
        path: &str,
        access: AccessKind,
        args: &[Dynamic],
        camera_condition: Option<CameraCondition>,
    ) -> Result<CallOutcome, Box<EvalAltResult>> {
        let catalog = crate::client_commands::catalog::catalog();
        let entry = catalog.resolve(path).ok_or_else(|| {
            values::error(
                "unknown_operation",
                path,
                "binding",
                "unknown catalog path",
                0,
            )
        })?;
        if !entry.spec.scriptable {
            return Err(values::error(
                "not_scriptable",
                path,
                "binding",
                "operation is host-only",
                0,
            ));
        }
        let permission = match access {
            AccessKind::Read => entry.spec.read,
            AccessKind::Write | AccessKind::Reset => entry.spec.write,
            AccessKind::Invoke => entry.spec.invoke,
        }
        .ok_or_else(|| {
            values::error(
                "unsupported_access",
                path,
                "binding",
                "access is unavailable",
                0,
            )
        })?;
        let mutation = !matches!(permission.effect, Effect::Query);
        if self.cancel.load(Ordering::Acquire) {
            return Err(values::error(
                "cancelled",
                path,
                "before_submit",
                "script was cancelled",
                0,
            ));
        }
        let mut ledger = self.ledger.lock().map_err(|_| {
            values::error(
                "host_failed",
                path,
                "before_submit",
                "ledger unavailable",
                0,
            )
        })?;
        if ledger.started.elapsed() >= Duration::from_secs(10) {
            return Err(values::error(
                "deadline",
                path,
                "before_submit",
                "script deadline elapsed",
                0,
            ));
        }
        if ledger.calls >= limits::OWNER_CALLS
            || (mutation && ledger.mutations >= limits::MUTATIONS)
        {
            return Err(values::error(
                "budget",
                path,
                "before_submit",
                "owner call budget exhausted",
                0,
            ));
        }
        if mutation && self.mutation_pending.load(Ordering::Acquire) {
            return Err(values::error(
                "mutation_pending",
                path,
                "before_submit",
                "await confirmation before another mutation",
                0,
            ));
        }
        let expected_arguments = if matches!(access, AccessKind::Read | AccessKind::Reset) {
            0
        } else {
            entry.spec.arguments.len()
        };
        if args.len() != expected_arguments {
            return Err(values::error(
                "invalid_arguments",
                path,
                "binding",
                "argument count differs from catalog",
                0,
            ));
        }
        let converted = args
            .iter()
            .enumerate()
            .map(|(i, value)| values::to_value(value, entry.spec.arguments[i].scalar, path))
            .collect::<Result<Vec<_>, _>>()?;
        let prepared =
            minimium_commands::syntax::bind::bind_values(catalog, path, access, &converted)
                .map_err(|e| values::error(e.code.as_str(), path, "binding", &e.reason, 0))?;
        ledger.calls += 1;
        ledger.operations.push(CallRecord {
            path: prepared.path.clone(),
            stage: "awaiting_host".into(),
            request_id: None,
            error_code: None,
            session: None,
            context: None,
            nonce: None,
        });
        let operation_index = ledger.operations.len() - 1;
        if mutation {
            ledger.mutations += 1;
            self.mutation_pending.store(true, Ordering::Release);
        }
        let wait = Duration::from_secs(10)
            .saturating_sub(ledger.started.elapsed())
            .min(Duration::from_secs(5));
        drop(ledger);
        let waiter = Arc::new(Waiter::new(
            Arc::clone(&self.ledger),
            operation_index,
            Arc::clone(&self.cancel),
            Arc::clone(&self.mutation_pending),
            mutation,
        ));
        let (reply, receiver) = mpsc::sync_channel(1);
        let request = HostRequest {
            run_id: self.run_id,
            path: prepared.path,
            access: prepared.kind,
            arguments: prepared.arguments.into_iter().map(|a| a.value).collect(),
            camera_condition,
            reply,
            waiter: Arc::clone(&waiter),
        };
        self.calls.try_send(request).map_err(|e| {
            if mutation {
                self.mutation_pending.store(false, Ordering::Release);
            }
            self.update_last("rejected", None, Some("host_unavailable"));
            match e {
                TrySendError::Full(_) => values::error(
                    "host_busy",
                    path,
                    "before_submit",
                    "host call queue is full",
                    0,
                ),
                TrySendError::Disconnected(_) => values::error(
                    "host_disconnected",
                    path,
                    "before_submit",
                    "host call queue closed",
                    0,
                ),
            }
        })?;
        let _wait_clock = super::metrics::WaitClock::start(&self.ledger);
        let result = receiver.recv_timeout(wait).map_err(|_| {
            self.update_last("unknown_after_timeout", None, Some("wait_timeout"));
            values::error(
                "wait_timeout",
                path,
                "await_host",
                "host response was not received",
                0,
            )
        })?;
        self.update_last(
            result.receipt.stage.label(),
            Some(result.identity.request_id),
            result
                .outcome
                .as_ref()
                .err()
                .map(|error| error.code.as_str()),
        );
        if result.receipt.stage != minimium_commands::results::Stage::Submitted && mutation {
            self.mutation_pending.store(false, Ordering::Release);
        }
        Ok(CallOutcome { result, waiter })
    }
}

pub(super) fn run(
    run_id: u64,
    path: PathBuf,
    calls: SyncSender<HostRequest>,
    cancel: Arc<AtomicBool>,
    ledger: Arc<Mutex<Ledger>>,
    cache: &mut super::cache::AstCache,
) {
    let result = execute(
        run_id,
        path,
        calls,
        Arc::clone(&cancel),
        Arc::clone(&ledger),
        cache,
    );
    if let Ok(mut state) = ledger.lock() {
        state.current_phase = "finished";
        state.phase = if cancel.load(Ordering::Acquire) {
            "cancelled"
        } else if result.is_ok() {
            "completed"
        } else {
            "failed"
        };
        match result {
            Ok(value) => {
                state.metrics.termination_code = Some("completed".into());
                state.result = Some(value);
            }
            Err(error) => {
                state.metrics.termination_code = Some(if cancel.load(Ordering::Acquire) {
                    "cancelled".into()
                } else {
                    error.split(':').next().unwrap_or("failed").into()
                });
                state.error = Some(error);
            }
        }
    }
}

fn execute(
    run_id: u64,
    path: PathBuf,
    calls: SyncSender<HostRequest>,
    cancel: Arc<AtomicBool>,
    ledger: Arc<Mutex<Ledger>>,
    cache: &mut super::cache::AstCache,
) -> Result<String, String> {
    set_phase(&ledger, "load");
    let started = Instant::now();
    let source = minimium_platform::files::read_bounded_file(&path, limits::SOURCE_BYTES)
        .map_err(|e| format!("script_file: {e}"));
    if let Ok(mut state) = ledger.lock() {
        state.metrics.load = Some(started.elapsed());
        state.metrics.source_bytes = source.as_ref().ok().map(Vec::len);
    }
    let source = source?;
    let source_text = std::str::from_utf8(&source).map_err(|e| format!("script_utf8: {e}"))?;
    check_run(&cancel, &ledger)?;
    set_phase(&ledger, "cache_lookup");
    let started = Instant::now();
    let key = super::cache::CacheKey::for_source(
        &source,
        crate::client_commands::catalog::catalog().fingerprint(),
    );
    let cached_ast = cache.get(&key, &source);
    if let Ok(mut state) = ledger.lock() {
        state.metrics.cache_lookup = Some(started.elapsed());
        state.metrics.cache_hit = Some(cached_ast.is_some());
        (
            state.metrics.cache_entries,
            state.metrics.cached_source_bytes,
        ) = {
            let (entries, bytes) = cache.occupancy();
            (Some(entries), Some(bytes))
        };
    }
    check_run(&cancel, &ledger)?;
    set_phase(&ledger, "adapter_setup");
    let started = Instant::now();
    let mut engine = Engine::new();
    engine
        .set_optimization_level(OptimizationLevel::None)
        .set_max_operations(limits::MAX_OPERATIONS)
        .set_max_call_levels(limits::MAX_CALL_LEVELS)
        .set_max_expr_depths(limits::MAX_EXPR_DEPTH, limits::MAX_FUNCTION_EXPR_DEPTH)
        .set_max_variables(limits::MAX_VARIABLES)
        .set_max_string_size(limits::MAX_STRING_BYTES)
        .set_max_array_size(limits::MAX_ARRAY_SIZE)
        .set_max_map_size(limits::MAX_MAP_SIZE);
    engine.disable_symbol("eval");
    let check_cancel = Arc::clone(&cancel);
    let check_ledger = Arc::clone(&ledger);
    engine.on_progress(move |_| {
        let expired = check_ledger.lock().is_ok_and(|state| {
            state.started.elapsed() >= Duration::from_secs(10)
                || state.output.len() >= limits::OUTPUT_BYTES
        });
        (check_cancel.load(Ordering::Acquire) || expired).then_some(Dynamic::UNIT)
    });
    let print_ledger = Arc::clone(&ledger);
    engine.on_print(move |text| append_output(&print_ledger, text));
    let debug_ledger = Arc::clone(&ledger);
    engine.on_debug(move |text, _, _| append_output(&debug_ledger, text));
    super::registration::register(
        &mut engine,
        Bridge {
            run_id,
            calls,
            cancel: Arc::clone(&cancel),
            ledger: Arc::clone(&ledger),
            mutation_pending: Arc::new(AtomicBool::new(false)),
        },
    );
    if let Ok(mut state) = ledger.lock() {
        state.metrics.adapter_setup = Some(started.elapsed());
    }
    check_run(&cancel, &ledger)?;
    let ast = if let Some(ast) = cached_ast {
        ast
    } else {
        set_phase(&ledger, "compile");
        let started = Instant::now();
        let compiled = engine
            .compile(source_text)
            .map_err(|e| format!("script_compile: {e}"));
        if let Ok(mut state) = ledger.lock() {
            state.metrics.compile = Some(started.elapsed());
        }
        let ast = compiled?;
        check_run(&cancel, &ledger)?;
        cache.insert(key, &source, ast.clone());
        if let Ok(mut state) = ledger.lock() {
            let (entries, bytes) = cache.occupancy();
            state.metrics.cache_entries = Some(entries);
            state.metrics.cached_source_bytes = Some(bytes);
        }
        ast
    };
    check_run(&cancel, &ledger)?;
    set_phase(&ledger, "evaluate");
    let started = Instant::now();
    let mut scope = Scope::new();
    let result = engine
        .eval_ast_with_scope::<Dynamic>(&mut scope, &ast)
        .map_err(|e| format!("script_eval: {e}"));
    if let Ok(mut state) = ledger.lock() {
        state.metrics.evaluate = Some(started.elapsed());
    }
    let result = result?;
    check_run(&cancel, &ledger)?;
    set_phase(&ledger, "format");
    let started = Instant::now();
    let text = result.to_string();
    if let Ok(mut state) = ledger.lock() {
        state.metrics.format = Some(started.elapsed());
        state.metrics.result_bytes = Some(text.len());
    }
    let output_len = ledger.lock().map_or(0, |state| state.output.len());
    if text.len().saturating_add(output_len) > limits::RESULT_BYTES {
        return Err("result_limit: aggregate script result exceeds 256 KiB".into());
    }
    Ok(text)
}

fn set_phase(ledger: &Arc<Mutex<Ledger>>, phase: &'static str) {
    if let Ok(mut state) = ledger.lock() {
        state.current_phase = phase;
    }
}

fn check_run(cancel: &AtomicBool, ledger: &Arc<Mutex<Ledger>>) -> Result<(), String> {
    if cancel.load(Ordering::Acquire) {
        return Err("script_cancelled: cancellation requested".into());
    }
    if ledger
        .lock()
        .is_ok_and(|state| state.started.elapsed() >= Duration::from_secs(10))
    {
        return Err("script_deadline: ten-second run deadline exceeded".into());
    }
    Ok(())
}

fn append_output(ledger: &Arc<Mutex<Ledger>>, text: &str) {
    if let Ok(mut state) = ledger.lock() {
        let available = limits::OUTPUT_BYTES.saturating_sub(state.output.len());
        let end = text
            .char_indices()
            .map(|(i, ch)| i + ch.len_utf8())
            .take_while(|i| *i <= available)
            .last()
            .unwrap_or(0);
        state.output.push_str(&text[..end]);
    }
}
