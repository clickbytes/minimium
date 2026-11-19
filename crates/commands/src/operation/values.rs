use std::collections::BTreeMap;

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(try_from = "f64")]
pub struct FiniteValue(f64);

impl TryFrom<f64> for FiniteValue {
    type Error = &'static str;
    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value).ok_or("expected finite value")
    }
}

impl FiniteValue {
    pub fn new(value: f64) -> Option<Self> {
        value.is_finite().then_some(Self(value))
    }

    pub fn get(self) -> f64 {
        self.0
    }
}

/// Domain codecs validate bounds and interpret decimal coefficients and ID widths.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub enum CommandValue {
    Unit,
    Bool(bool),
    I64(#[serde(with = "crate::admin::number")] i64),
    U64(#[serde(with = "crate::admin::number")] u64),
    U128(#[serde(with = "crate::admin::number")] u128),
    Decimal {
        #[serde(with = "crate::admin::number")]
        coefficient: i128,
        fractional_digits: u8,
    },
    Finite(FiniteValue),
    Enum(String),
    Text(String),
    InstantMs(#[serde(with = "crate::admin::number")] i64),
    Record(BTreeMap<String, CommandValue>),
    Page {
        rows: Vec<CommandValue>,
        next_cursor: Option<String>,
    },
}

impl CommandValue {
    pub fn decimal(mut coefficient: i128, mut fractional_digits: u8) -> Self {
        while fractional_digits > 0 && coefficient % 10 == 0 {
            coefficient /= 10;
            fractional_digits -= 1;
        }
        Self::Decimal {
            coefficient,
            fractional_digits,
        }
    }
    /// Display scalars in their public encoding; compound values use the stable RON codec.
    pub fn encode(&self) -> String {
        match self {
            Self::Enum(text) | Self::Text(text) => text.clone(),
            Self::Bool(value) => value.to_string(),
            Self::I64(value) | Self::InstantMs(value) => value.to_string(),
            Self::U64(value) => value.to_string(),
            Self::U128(value) => value.to_string(),
            Self::Finite(value) => value.get().to_string(),
            Self::Decimal {
                coefficient,
                fractional_digits,
            } => {
                let mut digits = coefficient.unsigned_abs().to_string();
                let scale = usize::from(*fractional_digits);
                if scale > 0 {
                    if digits.len() <= scale {
                        digits = format!("{}{}", "0".repeat(scale + 1 - digits.len()), digits);
                    }
                    digits.insert(digits.len() - scale, '.');
                }
                if *coefficient < 0 {
                    digits.insert(0, '-');
                }
                digits
            }
            other => ron::to_string(other).expect("validated operation value"),
        }
    }
}
