use super::SettingsStore;
use crate::client_commands::{
    property_spec, ClientPropertyKey as Key, ClientPropertyValue as Value,
};
use minimium_commands::operation::CommandValue;
use minimium_commands::results::property::{Freshness, ReadSource, StartupValue};
use minimium_commands::{PropertySource, PropertyValue};

impl SettingsStore {
    pub(crate) fn typed_property(
        &self,
        key: Key,
        encode: fn(&Value) -> CommandValue,
    ) -> PropertyValue<CommandValue> {
        let name = key.key();
        let record = self.records.get(name).expect("registered startup setting");
        let startup = self.startup.get(name).expect("registered startup setting");
        let default = property_spec(name)
            .and_then(|spec| spec.default)
            .map(|text| {
                crate::client_commands::parse_setting_value(name, text)
                    .expect("validated property default")
            })
            .map(|value| encode(&value));
        let requested = encode(&record.requested.typed);
        PropertyValue {
            lifecycle: property_spec(name).expect("registered property").lifecycle,
            requested: Some(requested.clone()),
            effective: record.effective.as_ref().map(|v| encode(&v.typed)),
            default,
            startup: Some(StartupValue {
                value: encode(&startup.requested.typed),
                source: minimium_commands::results::property::ConfigurationSource::from_source(
                    startup.source,
                )
                .expect("startup source"),
            }),
            session_override: (record.source == PropertySource::Session).then_some(requested),
            source: record.source,
            read_source: ReadSource::LocalOwner,
            revision: None,
            freshness: Freshness::Current,
            difference_reason: record.difference_reason.clone(),
        }
    }
}
