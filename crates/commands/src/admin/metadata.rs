use crate::operation::{CatalogEntry, CommandValue as V};
use serde_json::{json, Value};

/// Machine-readable catalog fields use the same declarations as both help formatters.
pub fn describe(entry: &CatalogEntry) -> V {
    let object = json!({"spec":entry.spec,"aliases":entry.aliases,"bindings":entry.bindings,
        "backend":if entry.command.is_some()||entry.property.is_some(){"legacy"}else{"catalog"}});
    metadata(object)
}
fn metadata(v: Value) -> V {
    match v {
        Value::Null => V::Unit,
        Value::Bool(b) => V::Bool(b),
        Value::String(s) => V::Text(s),
        Value::Number(n) => V::U64(
            n.as_u64()
                .expect("catalog metadata uses unsigned numeric fields"),
        ),
        Value::Array(a) => V::Page {
            rows: a.into_iter().map(metadata).collect(),
            next_cursor: None,
        },
        Value::Object(fields) => {
            V::Record(fields.into_iter().map(|(k, v)| (k, metadata(v))).collect())
        }
    }
}
