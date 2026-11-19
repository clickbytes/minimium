use super::{bridge::HostRequest, limits, values, CallRecord, Ledger};
use minimium_commands::operation::{CommandValue, Effect};
use minimium_commands::results::Payload;
use minimium_commands::syntax::canonical::AccessKind;
use rhai::{Array, Dynamic, Engine, EvalAltResult, ImmutableString, OptimizationLevel, Scope};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc::{self, SyncSender, TrySendError},
    Arc, Mutex,
};
use std::time::Duration;

#[derive(Clone)]
struct Bridge {
    calls: SyncSender<HostRequest>,
    cancel: Arc<AtomicBool>,
    ledger: Arc<Mutex<Ledger>>,
    mutation_pending: Arc<AtomicBool>,
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

    fn call(
        &self,
        path: &str,
        access: AccessKind,
        args: &[Dynamic],
    ) -> Result<minimium_commands::results::OperationResult, Box<EvalAltResult>> {
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
        if args.len() != entry.spec.arguments.len() && access != AccessKind::Read {
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
        });
        if mutation {
            ledger.mutations += 1;
            self.mutation_pending.store(true, Ordering::Release);
        }
        let wait = Duration::from_secs(10)
            .saturating_sub(ledger.started.elapsed())
            .min(Duration::from_secs(5));
        drop(ledger);
        let (reply, receiver) = mpsc::sync_channel(1);
        let request = HostRequest {
            path: prepared.path,
            access: prepared.kind,
            arguments: prepared.arguments.into_iter().map(|a| a.value).collect(),
            reply,
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
        Ok(result)
    }
}

pub(super) fn run(
    path: PathBuf,
    calls: SyncSender<HostRequest>,
    cancel: Arc<AtomicBool>,
    ledger: Arc<Mutex<Ledger>>,
) {
    let result = execute(path, calls, Arc::clone(&cancel), Arc::clone(&ledger));
    if let Ok(mut state) = ledger.lock() {
        state.phase = if cancel.load(Ordering::Acquire) {
            "cancelled"
        } else if result.is_ok() {
            "completed"
        } else {
            "failed"
        };
        match result {
            Ok(value) => state.result = Some(value),
            Err(error) => state.error = Some(error),
        }
    }
}

fn execute(
    path: PathBuf,
    calls: SyncSender<HostRequest>,
    cancel: Arc<AtomicBool>,
    ledger: Arc<Mutex<Ledger>>,
) -> Result<String, String> {
    let source = minimium_platform::files::read_bounded_file(&path, limits::SOURCE_BYTES)
        .map_err(|e| format!("script_file: {e}"))?;
    let source = std::str::from_utf8(&source).map_err(|e| format!("script_utf8: {e}"))?;
    let mut engine = Engine::new();
    engine
        .set_optimization_level(OptimizationLevel::None)
        .set_max_operations(20_000)
        .set_max_call_levels(16)
        .set_max_expr_depths(32, 16)
        .set_max_variables(256)
        .set_max_string_size(8192)
        .set_max_array_size(256)
        .set_max_map_size(64);
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
    register(
        &mut engine,
        Bridge {
            calls,
            cancel,
            ledger: Arc::clone(&ledger),
            mutation_pending: Arc::new(AtomicBool::new(false)),
        },
    );
    let ast = engine
        .compile(source)
        .map_err(|e| format!("script_compile: {e}"))?;
    let mut scope = Scope::new();
    let result = engine
        .eval_ast_with_scope::<Dynamic>(&mut scope, &ast)
        .map_err(|e| format!("script_eval: {e}"))?;
    let text = result.to_string();
    let output_len = ledger.lock().map_or(0, |state| state.output.len());
    if text.len().saturating_add(output_len) > limits::RESULT_BYTES {
        return Err("result_limit: aggregate script result exceeds 256 KiB".into());
    }
    Ok(text)
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

fn register(engine: &mut Engine, bridge: Bridge) {
    use values::{
        EntityId, ExactScale, InstanceId, OperationHandle, PropertySnapshot, Revision, ScriptRunId,
    };
    engine.register_type_with_name::<PropertySnapshot>("PropertySnapshot");
    engine.register_type_with_name::<OperationHandle>("OperationHandle");
    engine.register_type_with_name::<Revision>("Revision");
    engine.register_type_with_name::<InstanceId>("InstanceId");
    engine.register_type_with_name::<EntityId>("EntityId");
    engine.register_type_with_name::<ExactScale>("ExactScale");
    engine.register_type_with_name::<ScriptRunId>("ScriptRunId");
    engine.register_fn(
        "revision",
        |text: ImmutableString| -> Result<Revision, Box<EvalAltResult>> {
            text.parse::<u64>().map(Revision).map_err(|_| {
                values::error(
                    "invalid_value",
                    "revision",
                    "binding",
                    "expected exact u64",
                    0,
                )
            })
        },
    );
    engine.register_fn(
        "instance_id",
        |text: ImmutableString| -> Result<InstanceId, Box<EvalAltResult>> {
            text.parse::<u128>().map(InstanceId).map_err(|_| {
                values::error(
                    "invalid_value",
                    "instance_id",
                    "binding",
                    "expected exact u128",
                    0,
                )
            })
        },
    );
    engine.register_fn(
        "entity_id",
        |text: ImmutableString| -> Result<EntityId, Box<EvalAltResult>> {
            text.parse::<u64>().map(EntityId).map_err(|_| {
                values::error(
                    "invalid_value",
                    "entity_id",
                    "binding",
                    "expected exact u64",
                    0,
                )
            })
        },
    );
    engine.register_fn(
        "script_run_id",
        |text: ImmutableString| -> Result<ScriptRunId, Box<EvalAltResult>> {
            text.parse::<u64>().map(ScriptRunId).map_err(|_| {
                values::error(
                    "invalid_value",
                    "script_run_id",
                    "binding",
                    "expected exact u64",
                    0,
                )
            })
        },
    );
    engine.register_get("decimal", |id: &mut Revision| id.0.to_string());
    engine.register_get("decimal", |id: &mut InstanceId| id.0.to_string());
    engine.register_get("decimal", |id: &mut EntityId| id.0.to_string());
    engine.register_get("decimal", |id: &mut ScriptRunId| id.0.to_string());
    engine.register_get("revision", |s: &mut PropertySnapshot| {
        s.0.revision
            .map_or(Dynamic::UNIT, |v| Dynamic::from(Revision(v)))
    });
    engine.register_get("effective_available", |s: &mut PropertySnapshot| {
        s.0.effective.is_some()
    });
    engine.register_get("effective", |s: &mut PropertySnapshot| {
        s.0.effective
            .clone()
            .map_or(Dynamic::UNIT, values::to_dynamic)
    });
    engine.register_get("requested", |s: &mut PropertySnapshot| {
        s.0.requested
            .clone()
            .map_or(Dynamic::UNIT, values::to_dynamic)
    });
    engine.register_get("session_override", |s: &mut PropertySnapshot| {
        s.0.session_override
            .clone()
            .map_or(Dynamic::UNIT, values::to_dynamic)
    });
    engine.register_get("stage", |h: &mut OperationHandle| {
        h.0.receipt.stage.label().to_owned()
    });
    engine.register_get("request_id", |h: &mut OperationHandle| {
        ScriptRunId(h.0.identity.request_id)
    });
    engine.register_fn(
        "scale",
        |text: ImmutableString| -> Result<ExactScale, Box<EvalAltResult>> {
            let scale = minimium_environment::parse_scale_ppm(text.as_str())
                .map_err(|e| values::error("invalid_scale", "scale", "binding", &e, 0))?;
            let ppm = scale.get();
            Ok(ExactScale(CommandValue::Decimal {
                coefficient: i128::from(ppm),
                fractional_digits: 6,
            }))
        },
    );
    let get = bridge.clone();
    engine.register_fn(
        "f1_get",
        move |path: ImmutableString| -> Result<PropertySnapshot, Box<EvalAltResult>> {
            let result = get.call(path.as_str(), AccessKind::Read, &[])?;
            match result.outcome {
                Ok(Payload::Property(value)) => Ok(PropertySnapshot(value)),
                Err(error) => Err(values::error(
                    &error.code,
                    path.as_str(),
                    result.receipt.stage.label(),
                    &error.reason,
                    result.identity.request_id,
                )),
                _ => Err(values::error(
                    "untyped_result",
                    path.as_str(),
                    result.receipt.stage.label(),
                    "expected PropertySnapshot",
                    result.identity.request_id,
                )),
            }
        },
    );
    let set = bridge.clone();
    engine.register_fn(
        "f1_set",
        move |path: ImmutableString,
              value: Dynamic|
              -> Result<OperationHandle, Box<EvalAltResult>> {
            let result = set.call(path.as_str(), AccessKind::Write, &[value])?;
            if let Err(failure) = &result.outcome {
                return Err(values::error(
                    &failure.code,
                    path.as_str(),
                    result.receipt.stage.label(),
                    &failure.reason,
                    result.identity.request_id,
                ));
            }
            Ok(OperationHandle(result))
        },
    );
    engine.register_fn(
        "f1_call",
        move |path: ImmutableString, args: Array| -> Result<Dynamic, Box<EvalAltResult>> {
            let result = bridge.call(path.as_str(), AccessKind::Invoke, &args)?;
            if let Err(failure) = &result.outcome {
                return Err(values::error(
                    &failure.code,
                    path.as_str(),
                    result.receipt.stage.label(),
                    &failure.reason,
                    result.identity.request_id,
                ));
            }
            let entry = crate::client_commands::catalog::catalog()
                .resolve(path.as_str())
                .expect("bound script operation");
            if matches!(entry.spec.invoke.map(|a| a.effect), Some(Effect::Query)) {
                values::result_value(result)
            } else {
                Ok(Dynamic::from(OperationHandle(result)))
            }
        },
    );
}
