//! Typed values for client-owned input and presentation preferences.
use super::{ClientPropertyKey as Key, ClientPropertyValue as Value};
use minimium_commands::operation::{CommandValue, FiniteValue};

pub(crate) fn owns(key: Key) -> bool {
    matches!(
        key,
        Key::MouseSensitivity
            | Key::InputBindings
            | Key::SkyQuality
            | Key::SkyNames
            | Key::CloudDetail
            | Key::AtmosphereDebug
            | Key::MoonFlux
            | Key::ExposureEv
            | Key::TerrainDebug
            | Key::CloudDebug
            | Key::SkyDemo
    )
}

pub(crate) fn encode(value: &Value) -> CommandValue {
    match value {
        Value::MouseSensitivity(v) => finite(*v),
        Value::InputBindings(v) => CommandValue::Text(v.clone()),
        Value::SkyNames(v) | Value::MoonFlux(v) | Value::SkyDemo(v) => CommandValue::Bool(*v),
        Value::CloudDetail(v) => CommandValue::U64(u64::from(*v)),
        Value::ExposureEv(Some(v)) => finite(*v),
        Value::ExposureEv(None) => CommandValue::Enum("auto".into()),
        Value::SkyQuality(_)
        | Value::AtmosphereDebug(_)
        | Value::TerrainDebug(_)
        | Value::CloudDebug(_) => CommandValue::Enum(value.encode()),
        _ => unreachable!("local environment preference only"),
    }
}

fn finite(value: f32) -> CommandValue {
    CommandValue::Finite(FiniteValue::new(f64::from(value)).expect("validated finite setting"))
}
