use super::SettingsStore;
use crate::client_commands::{local_environment, property_spec, ClientPropertyKey as Key};
use minimium_commands::operation::CommandValue;
use minimium_commands::results::property::{Freshness, ReadSource, StartupValue};
use minimium_commands::{PropertySource, PropertyValue};

impl SettingsStore {
    pub(crate) fn local_environment_property(&self, key: Key) -> PropertyValue<CommandValue> {
        assert!(local_environment::owns(key));
        let name = key.key();
        let record = self.records.get(name).expect("registered startup setting");
        let startup = self.startup.get(name).expect("registered startup setting");
        let default = property_spec(name)
            .and_then(|spec| spec.default)
            .map(|text| {
                crate::client_commands::parse_setting_value(name, text)
                    .expect("validated property default")
            })
            .map(|value| local_environment::encode(&value));
        let requested = local_environment::encode(&record.requested.typed);
        PropertyValue {
            lifecycle: property_spec(name).expect("registered property").lifecycle,
            requested: Some(requested.clone()),
            effective: record
                .effective
                .as_ref()
                .map(|v| local_environment::encode(&v.typed)),
            default,
            startup: Some(StartupValue {
                value: local_environment::encode(&startup.requested.typed),
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
