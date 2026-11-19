use minimium_commands::operation::{CommandValue, ScalarType};
use minimium_commands::results::{OperationResult, Payload};
use rhai::{Dynamic, EvalAltResult, ImmutableString, Map, Position};

#[derive(Clone)]
pub(crate) struct Revision(pub(crate) u64, pub(crate) Option<u64>);
#[derive(Clone)]
pub(crate) struct InstanceId(pub(crate) u128);
#[derive(Clone)]
pub(crate) struct EntityId(pub(crate) u64);
#[derive(Clone)]
pub(crate) struct ScriptRunId(pub(crate) u64);
#[derive(Clone)]
pub(crate) struct ExactScale(pub(crate) CommandValue);
#[derive(Clone)]
pub(crate) struct OperationHandle {
    pub(crate) submitted: OperationResult,
    pub(crate) waiter: std::sync::Arc<super::bridge::Waiter>,
}
#[derive(Clone)]
pub(crate) struct ScriptReceipt(pub(crate) OperationResult);
#[derive(Clone)]
pub(crate) struct PropertySnapshot {
    pub(crate) value: minimium_commands::PropertyValue<CommandValue>,
    pub(crate) operation: minimium_commands::operation::OperationId,
    pub(crate) settings_base: Option<u64>,
}

pub(crate) fn error(
    code: &str,
    operation: &str,
    stage: &str,
    cause: &str,
    correlation: u64,
) -> Box<EvalAltResult> {
    let code = match code {
        "client_runtime.admin_control_unavailable" => "AdminControlUnavailable",
        "conflict" | "revision_conflict" | "settings_base_changed" => "Conflict",
        other => other,
    };
    let mut map = Map::new();
    for (key, value) in [
        ("code", code),
        ("operation", operation),
        ("stage", stage),
        ("cause", cause),
    ] {
        map.insert(key.into(), Dynamic::from(value.to_owned()));
    }
    map.insert(
        "correlation".into(),
        Dynamic::from(ScriptRunId(correlation)),
    );
    Box::new(EvalAltResult::ErrorRuntime(
        Dynamic::from_map(map),
        Position::NONE,
    ))
}

pub(crate) fn to_value(
    value: &Dynamic,
    scalar: ScalarType,
    operation: &str,
) -> Result<CommandValue, Box<EvalAltResult>> {
    if value.is::<ExactScale>() {
        return Ok(value.clone_cast::<ExactScale>().0);
    }
    if value.is::<Revision>() {
        return Ok(CommandValue::U64(value.clone_cast::<Revision>().0));
    }
    if value.is::<InstanceId>() {
        return Ok(CommandValue::U128(value.clone_cast::<InstanceId>().0));
    }
    if value.is::<EntityId>() {
        return Ok(CommandValue::U64(value.clone_cast::<EntityId>().0));
    }
    if value.is::<ScriptRunId>() {
        return Ok(CommandValue::U64(value.clone_cast::<ScriptRunId>().0));
    }
    if value.is::<bool>() {
        return Ok(CommandValue::Bool(value.clone_cast::<bool>()));
    }
    if value.is::<rhai::INT>() {
        let n = value.clone_cast::<rhai::INT>();
        return Ok(match scalar {
            ScalarType::U64 | ScalarType::NonzeroU64 | ScalarType::OptionalU64 => {
                CommandValue::U64(u64::try_from(n).map_err(|_| {
                    error(
                        "invalid_value",
                        operation,
                        "binding",
                        "expected nonnegative u64",
                        0,
                    )
                })?)
            }
            ScalarType::U128 | ScalarType::NonzeroU128 => {
                CommandValue::U128(u128::try_from(n).map_err(|_| {
                    error(
                        "invalid_value",
                        operation,
                        "binding",
                        "expected nonnegative u128",
                        0,
                    )
                })?)
            }
            _ => CommandValue::I64(n),
        });
    }
    if value.is::<rhai::FLOAT>() {
        let number = value.clone_cast::<rhai::FLOAT>();
        let finite = minimium_commands::operation::FiniteValue::new(number).ok_or_else(|| {
            error(
                "invalid_value",
                operation,
                "binding",
                "expected finite number",
                0,
            )
        })?;
        return Ok(CommandValue::Finite(finite));
    }
    if value.is::<ImmutableString>() {
        let text = value.clone_cast::<ImmutableString>().to_string();
        if text.len() > 8192 {
            return Err(error(
                "result_limit",
                operation,
                "binding",
                "string exceeds 8 KiB",
                0,
            ));
        }
        return Ok(match scalar {
            ScalarType::Text => CommandValue::Text(text),
            _ => CommandValue::Enum(text),
        });
    }
    Err(error(
        "invalid_value",
        operation,
        "binding",
        "unsupported Rhai value type",
        0,
    ))
}

pub(crate) fn to_dynamic(value: CommandValue) -> Dynamic {
    match value {
        CommandValue::Unit => Dynamic::UNIT,
        CommandValue::Bool(v) => Dynamic::from(v),
        CommandValue::I64(v) | CommandValue::InstantMs(v) => Dynamic::from(v),
        CommandValue::U64(v) => Dynamic::from(Revision(v, None)),
        CommandValue::U128(v) => Dynamic::from(InstanceId(v)),
        CommandValue::Finite(v) => Dynamic::from(v.get()),
        CommandValue::Decimal {
            coefficient,
            fractional_digits,
        } => Dynamic::from(
            CommandValue::Decimal {
                coefficient,
                fractional_digits,
            }
            .encode(),
        ),
        CommandValue::Text(v) | CommandValue::Enum(v) => Dynamic::from(v),
        CommandValue::Record(fields) => {
            let mut map = Map::new();
            for (key, value) in fields {
                map.insert(key.into(), to_dynamic(value));
            }
            Dynamic::from_map(map)
        }
        CommandValue::Page { rows, next_cursor } => {
            let mut map = Map::new();
            map.insert(
                "rows".into(),
                Dynamic::from_array(rows.into_iter().map(to_dynamic).collect()),
            );
            map.insert(
                "next_cursor".into(),
                next_cursor.map_or(Dynamic::UNIT, Dynamic::from),
            );
            Dynamic::from_map(map)
        }
    }
}

pub(crate) fn result_value(result: OperationResult) -> Result<Dynamic, Box<EvalAltResult>> {
    let operation = result.identity.operation.0.clone();
    let operation_id = result.identity.operation.clone();
    let correlation = result.identity.request_id;
    let settings_base = result.receipt.settings_base;
    match result.outcome {
        Err(failure) => Err(error(
            &failure.code,
            &operation,
            result.receipt.stage.label(),
            &failure.reason,
            correlation,
        )),
        Ok(Payload::Value(value)) => Ok(to_dynamic(value)),
        Ok(Payload::Property(value)) => Ok(Dynamic::from(PropertySnapshot {
            value,
            operation: operation_id,
            settings_base,
        })),
        Ok(Payload::LegacyText(_)) => Err(error(
            "untyped_result",
            &operation,
            result.receipt.stage.label(),
            "operation has no typed result",
            correlation,
        )),
    }
}
