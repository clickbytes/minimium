use super::{property_spec, ClientPropertyCommand, ClientPropertyKey, ClientPropertyValue};
use crate::environment::EnvironmentFrameInput;
use minimium_commands::{
    results::{
        property::{Freshness, ReadSource},
        Payload,
    },
    PropertySource, PropertyValue,
};
use minimium_environment::clock_command::ClockWrite;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ClockAction {
    Read(ClientPropertyKey),
    Status,
    InitialConfig,
    Write(ClockWrite),
    Scale(String),
}

impl ClockAction {
    pub(super) fn from_property(command: &ClientPropertyCommand) -> Option<Self> {
        use ClientPropertyValue as Value;
        Some(match command {
            ClientPropertyCommand::Get(key)
                if minimium_environment::console::live_clock_field(key.key()) =>
            {
                Self::Read(*key)
            }
            ClientPropertyCommand::Set(Value::WorldTimeScale(value)) => Self::Scale(value.clone()),
            ClientPropertyCommand::Set(Value::WorldTimeMode(mode)) => {
                Self::Write(ClockWrite::Mode(*mode))
            }
            ClientPropertyCommand::Set(Value::WorldTimeOfflinePolicy(policy)) => {
                Self::Write(ClockWrite::Offline(*policy))
            }
            _ => return None,
        })
    }
}

pub(crate) fn property(
    key: ClientPropertyKey,
    frame: EnvironmentFrameInput,
) -> Option<PropertyValue<minimium_commands::operation::CommandValue>> {
    if !minimium_environment::console::live_clock_field(key.key()) {
        return None;
    }
    let value = frame.clock_property(key.key());
    let demo =
        frame.quality == minimium_client_runtime::environment::SynchronizationQuality::LocalDemo;
    let (source, read_source, reason) = if value.is_none() {
        (
            PropertySource::Unavailable,
            ReadSource::Unavailable,
            "clock sample unavailable",
        )
    } else if demo {
        (PropertySource::Runtime, ReadSource::LocalOwner, "LocalDemo")
    } else {
        (
            PropertySource::Replica,
            ReadSource::Replica,
            frame.quality.label(),
        )
    };
    Some(PropertyValue {
        lifecycle: property_spec(key.key())?.lifecycle,
        startup: None,
        session_override: None,
        revision: frame
            .committed
            .map(|s| s.revision().get())
            .or(frame.revision),
        freshness: if value.is_some() {
            Freshness::Sampled
        } else {
            Freshness::Unknown
        },
        requested: value.clone(),
        effective: value,
        default: None,
        source,
        read_source,
        difference_reason: Some(reason.into()),
    })
}

pub(crate) fn read(action: &ClockAction, frame: EnvironmentFrameInput) -> Option<Payload> {
    match action {
        ClockAction::Read(key) => property(*key, frame)
            .filter(|p| p.effective.is_some())
            .map(Payload::Property),
        ClockAction::Status => frame.clock_status_value().map(Payload::Value),
        _ => None,
    }
}
