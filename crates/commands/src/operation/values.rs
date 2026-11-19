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
    I64(i64),
    U64(u64),
    U128(u128),
    Decimal {
        coefficient: i128,
        fractional_digits: u8,
    },
    Finite(FiniteValue),
    Enum(String),
    Text(String),
    InstantMs(i64),
    Record(BTreeMap<String, CommandValue>),
    Page {
        rows: Vec<CommandValue>,
        next_cursor: Option<String>,
    },
}

impl CommandValue {
    /// Display scalars in their public encoding; compound values use the stable RON codec.
    pub fn encode(&self) -> String {
        match self {
            Self::Enum(text) | Self::Text(text) => text.clone(),
            Self::Bool(value) => value.to_string(),
            Self::I64(value) | Self::InstantMs(value) => value.to_string(),
            Self::U64(value) => value.to_string(),
            Self::U128(value) => value.to_string(),
            Self::Finite(value) => value.get().to_string(),
            other => ron::to_string(other).expect("validated operation value"),
        }
    }
}
