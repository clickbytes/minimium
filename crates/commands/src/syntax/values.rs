use crate::operation::{CommandValue as Value, FiniteValue, ScalarType};

fn integer(text: &str) -> Option<u128> {
    if let Some(hex) = text.strip_prefix("0x") {
        u128::from_str_radix(hex, 16).ok()
    } else {
        text.parse().ok()
    }
}

fn decimal_parts(text: &str) -> Option<(&str, &str)> {
    let (whole, frac) = text.split_once('.').unwrap_or((text, ""));
    let digits = whole.strip_prefix('-').unwrap_or(whole);
    if digits.is_empty()
        || !digits
            .bytes()
            .chain(frac.bytes())
            .all(|c| c.is_ascii_digit())
        || text.ends_with('.')
    {
        return None;
    }
    Some((whole, frac))
}

fn decimal(text: &str) -> Option<Value> {
    let (whole, frac) = decimal_parts(text)?;
    let coefficient = format!("{whole}{frac}").parse::<i128>().ok()?;
    Some(Value::decimal(coefficient, u8::try_from(frac.len()).ok()?))
}

pub(super) fn bind_value(
    scalar: ScalarType,
    text: &mut String,
    quoted: bool,
    legacy: bool,
) -> Option<Value> {
    let t = text.as_str();
    use ScalarType::*;
    if !legacy {
        let integer_digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        let valid = match scalar {
            Integer => integer_digits(t.strip_prefix('-').unwrap_or(t)),
            U8 | U16 | U64 | U128 | NonzeroU64 | NonzeroU128 => {
                integer_digits(t)
                    || (quoted
                        && matches!(scalar, U128 | NonzeroU128 | NonzeroU64)
                        && t.strip_prefix("0x").is_some_and(|s| {
                            !s.is_empty() && s.bytes().all(|b| b.is_ascii_hexdigit())
                        }))
            }
            FiniteF32 | FiniteF64 | Decimal => decimal_parts(t).is_some(),
            AutoOrFiniteF32 => t == "auto" || decimal_parts(t).is_some(),
            OptionalU64 => t == "none" || integer_digits(t),
            InstantMs => quoted || integer_digits(t.strip_prefix('-').unwrap_or(t)),
            Text | Bool => true,
            Token => {
                quoted
                    || (!t.is_empty()
                        && t.bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'))
            }
        };
        if !valid {
            return None;
        }
    }
    let v = match scalar {
        Text => {
            if !quoted && !legacy {
                return None;
            }
            Value::Text(t.into())
        }
        InstantMs => match t.parse::<i64>() {
            Ok(ms) => Value::InstantMs(ms),
            Err(_) if quoted || legacy => Value::Text(t.into()),
            Err(_) => return None,
        },
        Integer => Value::I64(t.parse().ok()?),
        NonzeroU128 | NonzeroU64 => {
            let id = integer(t)?;
            if id == 0 {
                return None;
            }
            *text = id.to_string();
            if scalar == NonzeroU64 {
                Value::U64(id.try_into().ok()?)
            } else {
                Value::U128(id)
            }
        }
        Bool => match t {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            "on" if legacy => {
                *text = "true".into();
                Value::Bool(true)
            }
            "off" if legacy => {
                *text = "false".into();
                Value::Bool(false)
            }
            _ => return None,
        },
        U8 => Value::U64(t.parse::<u8>().ok()?.into()),
        U16 => Value::U64(t.parse::<u16>().ok()?.into()),
        U64 => Value::U64(t.parse().ok()?),
        U128 => Value::U128(integer(t)?),
        Decimal => decimal(t)?,
        FiniteF32 | FiniteF64 => {
            let f = if scalar == FiniteF32 {
                f64::from(t.parse::<f32>().ok()?)
            } else {
                t.parse().ok()?
            };
            Value::Finite(FiniteValue::new(f)?)
        }
        Token => Value::Enum(t.into()),
        OptionalU64 => {
            if t == "none" {
                Value::Unit
            } else {
                Value::U64(t.parse().ok()?)
            }
        }
        AutoOrFiniteF32 => {
            if t == "auto" {
                Value::Enum(t.into())
            } else {
                Value::Finite(FiniteValue::new(t.parse::<f32>().ok()? as f64)?)
            }
        }
    };
    Some(v)
}
