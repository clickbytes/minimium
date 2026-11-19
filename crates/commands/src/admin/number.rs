//! Decimal strings preserve exact identities for clients using binary64 JSON numbers.
use serde::{de::Error, Deserialize, Deserializer, Serializer};
use std::{fmt::Display, str::FromStr};
pub fn serialize<T: Display, S: Serializer>(v: &T, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&v.to_string())
}
pub fn deserialize<'de, T: FromStr, D: Deserializer<'de>>(d: D) -> Result<T, D::Error>
where
    T::Err: Display,
{
    let text = String::deserialize(d)?;
    text.parse().map_err(D::Error::custom)
}
pub mod optional {
    use super::*;
    pub fn serialize<S: Serializer>(v: &Option<u64>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(n) => s.serialize_some(&n.to_string()),
            None => s.serialize_none(),
        }
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
        Option::<String>::deserialize(d)?
            .map(|s| s.parse().map_err(D::Error::custom))
            .transpose()
    }
}
