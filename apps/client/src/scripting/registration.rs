use super::{bindings::Bridge, bridge::CameraCondition, values};
use minimium_commands::operation::{CommandValue, Effect};
use minimium_commands::results::Payload;
use minimium_commands::results::Stage;
use minimium_commands::syntax::canonical::AccessKind;
use rhai::{Array, Dynamic, Engine, EvalAltResult, ImmutableString};
use std::time::Duration;

pub(super) fn register(engine: &mut Engine, bridge: Bridge) {
    use values::{
        EntityId, ExactScale, InstanceId, OperationHandle, PropertySnapshot, Revision,
        ScriptReceipt, ScriptRunId,
    };
    engine.register_type_with_name::<PropertySnapshot>("PropertySnapshot");
    engine.register_type_with_name::<OperationHandle>("OperationHandle");
    engine.register_type_with_name::<ScriptReceipt>("OperationReceipt");
    engine.register_type_with_name::<Revision>("Revision");
    engine.register_type_with_name::<InstanceId>("InstanceId");
    engine.register_type_with_name::<EntityId>("EntityId");
    engine.register_type_with_name::<ExactScale>("ExactScale");
    engine.register_type_with_name::<ScriptRunId>("ScriptRunId");
    engine.register_fn(
        "revision",
        |text: ImmutableString| -> Result<Revision, Box<EvalAltResult>> {
            text.parse::<u64>().map(|v| Revision(v, None)).map_err(|_| {
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
        s.value.revision.map_or(Dynamic::UNIT, |v| {
            Dynamic::from(Revision(v, s.settings_base))
        })
    });
    engine.register_get("effective_available", |s: &mut PropertySnapshot| {
        s.value.effective.is_some()
    });
    engine.register_get("effective", |s: &mut PropertySnapshot| {
        s.value
            .effective
            .clone()
            .map_or(Dynamic::UNIT, values::to_dynamic)
    });
    engine.register_get("requested", |s: &mut PropertySnapshot| {
        s.value
            .requested
            .clone()
            .map_or(Dynamic::UNIT, values::to_dynamic)
    });
    engine.register_get("session_override", |s: &mut PropertySnapshot| {
        s.value
            .session_override
            .clone()
            .map_or(Dynamic::UNIT, values::to_dynamic)
    });
    engine.register_get("operation_id", |s: &mut PropertySnapshot| {
        s.operation.0.clone()
    });
    engine.register_get("factory_default", |s: &mut PropertySnapshot| {
        s.value
            .default
            .clone()
            .map_or(Dynamic::UNIT, values::to_dynamic)
    });
    engine.register_get("startup", |s: &mut PropertySnapshot| {
        s.value
            .startup
            .as_ref()
            .map_or(Dynamic::UNIT, |v| values::to_dynamic(v.value.clone()))
    });
    engine.register_get("startup_source", |s: &mut PropertySnapshot| {
        s.value
            .startup
            .as_ref()
            .map_or("unavailable", |v| v.source.label())
            .to_owned()
    });
    engine.register_get("source", |s: &mut PropertySnapshot| {
        format!("{:?}", s.value.source)
    });
    engine.register_get("read_source", |s: &mut PropertySnapshot| {
        match s.value.read_source {
            minimium_commands::results::property::ReadSource::LocalOwner => "local_owner",
            minimium_commands::results::property::ReadSource::OwnerSnapshot => "owner_snapshot",
            minimium_commands::results::property::ReadSource::Replica => "replica",
            minimium_commands::results::property::ReadSource::Unavailable => "unavailable",
        }
        .to_owned()
    });
    engine.register_get("freshness", |s: &mut PropertySnapshot| {
        format!("{:?}", s.value.freshness).to_lowercase()
    });
    engine.register_get("settings_base", |s: &mut PropertySnapshot| {
        s.settings_base
            .map_or(Dynamic::UNIT, |v| Dynamic::from(Revision(v, None)))
    });
    engine.register_get("stage", |h: &mut OperationHandle| {
        h.submitted.receipt.stage.label().to_owned()
    });
    engine.register_get("request_id", |h: &mut OperationHandle| {
        ScriptRunId(h.submitted.identity.request_id)
    });
    engine.register_get("property_revision", |h: &mut OperationHandle| {
        h.submitted.receipt.revision.map_or(Dynamic::UNIT, |v| {
            Dynamic::from(Revision(v, h.submitted.receipt.settings_base))
        })
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
            let result = get.call(path.as_str(), AccessKind::Read, &[], None)?.result;
            match result.outcome {
                Ok(Payload::Property(value)) => Ok(PropertySnapshot {
                    value,
                    operation: result.identity.operation,
                    settings_base: result.receipt.settings_base,
                }),
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
            let call = set.call(path.as_str(), AccessKind::Write, &[value], None)?;
            let result = call.result;
            if let Err(failure) = &result.outcome {
                return Err(values::error(
                    &failure.code,
                    path.as_str(),
                    result.receipt.stage.label(),
                    &failure.reason,
                    result.identity.request_id,
                ));
            }
            Ok(OperationHandle {
                submitted: result,
                waiter: call.waiter,
            })
        },
    );
    let conditional = bridge.clone();
    engine.register_fn(
        "f1_set_if",
        move |path: ImmutableString,
              value: Dynamic,
              expected: Revision|
              -> Result<OperationHandle, Box<EvalAltResult>> {
            if path.as_str() != "client.camera.fov" {
                return Err(values::error(
                    "unsupported_cas",
                    path.as_str(),
                    "binding",
                    "conditional write is supported by the camera FOV owner",
                    0,
                ));
            }
            let base = expected.1.ok_or_else(|| {
                values::error(
                    "missing_base",
                    path.as_str(),
                    "binding",
                    "revision has no settings base identity",
                    0,
                )
            })?;
            let call = conditional.call(
                path.as_str(),
                AccessKind::Write,
                &[value],
                Some(CameraCondition {
                    expected_base: base,
                    expected_revision: expected.0,
                }),
            )?;
            let result = call.result;
            if let Err(failure) = &result.outcome {
                return Err(values::error(
                    &failure.code,
                    path.as_str(),
                    result.receipt.stage.label(),
                    &failure.reason,
                    result.identity.request_id,
                ));
            }
            Ok(OperationHandle {
                submitted: result,
                waiter: call.waiter,
            })
        },
    );
    let restore = bridge.clone();
    engine.register_fn(
        "f1_restore",
        move |snapshot: PropertySnapshot,
              expected: Revision|
              -> Result<OperationHandle, Box<EvalAltResult>> {
            let path = snapshot.operation.0.as_str();
            if path != "client.camera.fov" {
                return Err(values::error(
                    "unsupported_restore",
                    path,
                    "binding",
                    "restore is supported by the camera FOV owner",
                    0,
                ));
            }
            let base = snapshot.settings_base.ok_or_else(|| {
                values::error(
                    "missing_base",
                    path,
                    "binding",
                    "snapshot has no settings base identity",
                    0,
                )
            })?;
            if expected.1 != Some(base) {
                return Err(values::error(
                    "settings_base_changed",
                    path,
                    "binding",
                    "revision belongs to a different settings base",
                    0,
                ));
            }
            let (access, arguments) = match snapshot.value.session_override {
                Some(value) => (AccessKind::Write, vec![values::to_dynamic(value)]),
                None => (AccessKind::Reset, Vec::new()),
            };
            let call = restore.call(
                path,
                access,
                &arguments,
                Some(CameraCondition {
                    expected_base: base,
                    expected_revision: expected.0,
                }),
            )?;
            let result = call.result;
            if let Err(failure) = &result.outcome {
                return Err(values::error(
                    &failure.code,
                    path,
                    result.receipt.stage.label(),
                    &failure.reason,
                    result.identity.request_id,
                ));
            }
            Ok(OperationHandle {
                submitted: result,
                waiter: call.waiter,
            })
        },
    );
    engine.register_fn(
        "f1_call",
        move |path: ImmutableString, args: Array| -> Result<Dynamic, Box<EvalAltResult>> {
            let call = bridge.call(path.as_str(), AccessKind::Invoke, &args, None)?;
            let result = call.result;
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
                Ok(Dynamic::from(OperationHandle {
                    submitted: result,
                    waiter: call.waiter,
                }))
            }
        },
    );
    engine.register_get("stage", |r: &mut ScriptReceipt| {
        r.0.receipt.stage.label().to_owned()
    });
    engine.register_get("property_revision", |r: &mut ScriptReceipt| {
        r.0.receipt.revision.map_or(Dynamic::UNIT, |v| {
            Dynamic::from(Revision(v, r.0.receipt.settings_base))
        })
    });
    engine.register_get("request_id", |r: &mut ScriptReceipt| {
        ScriptRunId(r.0.identity.request_id)
    });
    engine.register_get("correlation_nonce", |r: &mut ScriptReceipt| {
        r.0.identity
            .correlation_nonce
            .map_or(Dynamic::UNIT, |v| Dynamic::from(Revision(v, None)))
    });
    engine.register_get("tick", |r: &mut ScriptReceipt| match &r.0.outcome {
        Ok(Payload::Value(CommandValue::Record(fields))) => fields
            .get("tick")
            .cloned()
            .map_or(Dynamic::UNIT, values::to_dynamic),
        _ => Dynamic::UNIT,
    });
    engine.register_fn(
        "f1_wait_committed",
        |value: Dynamic, timeout_ms: rhai::INT| -> Result<ScriptReceipt, Box<EvalAltResult>> {
            let Some(handle) = value.try_cast::<OperationHandle>() else {
                return Err(values::error(
                    "invalid_handle",
                    "f1_wait_committed",
                    "binding",
                    "expected an operation handle",
                    0,
                ));
            };
            let operation = handle.submitted.identity.operation.0.clone();
            if !(0..=5000).contains(&timeout_ms) {
                return Err(values::error(
                    "invalid_timeout",
                    &operation,
                    "binding",
                    "timeout must be 0..5000 ms",
                    handle.submitted.identity.request_id,
                ));
            }
            let result = if handle.submitted.receipt.stage == Stage::Submitted {
                handle
                    .waiter
                    .wait(Duration::from_millis(timeout_ms as u64))
                    .map_err(|code| {
                        let stage = if code == "wait_timeout" {
                            "unknown_after_timeout"
                        } else {
                            "await_owner"
                        };
                        values::error(
                            code,
                            &operation,
                            stage,
                            "owner outcome is not confirmed",
                            handle.submitted.identity.request_id,
                        )
                    })?
            } else {
                handle.submitted
            };
            if let Err(failure) = &result.outcome {
                return Err(values::error(
                    &failure.code,
                    &operation,
                    result.receipt.stage.label(),
                    &failure.reason,
                    result.identity.request_id,
                ));
            }
            if !matches!(
                result.receipt.stage,
                Stage::OwnerCommitted | Stage::Realized | Stage::Durable
            ) {
                return Err(values::error(
                    "not_committed",
                    &operation,
                    result.receipt.stage.label(),
                    "owner commitment has not been confirmed",
                    result.identity.request_id,
                ));
            }
            Ok(ScriptReceipt(result))
        },
    );
}
