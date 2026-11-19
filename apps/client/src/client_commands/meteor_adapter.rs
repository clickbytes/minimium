//! Typed local meteor and audio preferences, independent of the desktop GPU.
use super::{
    ClientPropertyKey as Key, ClientPropertyValue as Value, FrontendAction, MeteorDebug,
    MeteorLightDebug, MeteorPreset, PresentationOperation,
};
use minimium_commands::operation::{CommandValue, FiniteValue};
#[cfg(feature = "desktop")]
use minimium_commands::PropertyValue;

#[cfg(feature = "desktop")]
pub(crate) struct MeteorDiagnosticReport {
    pub property: PropertyValue<CommandValue>,
    pub snapshot: Option<CommandValue>,
}

pub(crate) fn owns(key: Key) -> bool {
    matches!(
        key,
        Key::MeteorEnabled
            | Key::MeteorRate
            | Key::MeteorQuality
            | Key::MeteorSeed
            | Key::MeteorAudio
            | Key::MeteorVolume
    )
}

pub(crate) fn encode(value: &Value) -> CommandValue {
    match value {
        Value::MeteorEnabled(v) | Value::MeteorAudio(v) => CommandValue::Bool(*v),
        Value::MeteorRate(v) => CommandValue::Finite(FiniteValue::new(*v).expect("validated rate")),
        Value::MeteorQuality(v) => CommandValue::Enum(v.label().into()),
        Value::MeteorSeed(Some(v)) => CommandValue::U64(*v),
        Value::MeteorSeed(None) => CommandValue::Enum("none".into()),
        Value::MeteorVolume(v) => {
            CommandValue::Finite(FiniteValue::new(f64::from(*v)).expect("validated volume"))
        }
        _ => unreachable!("meteor preference only"),
    }
}

#[cfg_attr(not(feature = "desktop"), allow(dead_code))]
#[derive(Clone, Copy, Debug)]
pub(crate) enum MeteorOperation {
    Status,
    Spawn(MeteorPreset, Option<u64>),
    Clear,
    Debug(MeteorDebug),
    Light(MeteorLightDebug),
    DebugRead,
    LightRead,
}

impl MeteorOperation {
    pub(crate) fn from_action(action: &FrontendAction) -> Option<Self> {
        let FrontendAction::Presentation(operation) = action else {
            return None;
        };
        Some(match operation {
            PresentationOperation::MeteorStatus => Self::Status,
            PresentationOperation::MeteorSpawn(preset, seed) => Self::Spawn(*preset, *seed),
            PresentationOperation::MeteorClear => Self::Clear,
            PresentationOperation::MeteorDebug(mode) => Self::Debug(*mode),
            PresentationOperation::MeteorLight(mode) => Self::Light(*mode),
            PresentationOperation::MeteorDebugRead => Self::DebugRead,
            PresentationOperation::MeteorLightRead => Self::LightRead,
            _ => return None,
        })
    }
}
