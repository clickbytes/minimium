use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FiniteValue(f64);

impl FiniteValue {
    pub fn new(value: f64) -> Option<Self> {
        value.is_finite().then_some(Self(value))
    }

    pub fn get(self) -> f64 {
        self.0
    }
}

/// Domain codecs validate bounds and interpret decimal coefficients and ID widths.
#[derive(Clone, Debug, PartialEq)]
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
