mod locale;
mod revisions;
pub(crate) use revisions::{PropertyRevision, SettingsBaseId};
mod startup;
use crate::client_commands::ClientPropertyValue;
use minimium_commands::PropertySource;
pub(crate) use startup::StartupSettings;
use std::collections::BTreeMap;

#[cfg(feature = "desktop")]
pub(crate) struct ClientSettings {
    pub(crate) locale: minimium_localization::Locale,
    pub(crate) local_server_config: minimium_client_runtime::LocalServerLaunchConfig,
    pub(crate) require_earth: bool,
    pub(crate) demo: bool,
    pub(crate) config_file_settings: BTreeMap<String, String>,
    pub(crate) values: SettingsStore,
}

#[cfg(feature = "desktop")]
impl ClientSettings {
    pub(crate) fn new(
        locale: minimium_localization::Locale,
        local_server_config: minimium_client_runtime::LocalServerLaunchConfig,
        require_earth: bool,
        demo: bool,
        config_file_settings: BTreeMap<String, String>,
        values: SettingsStore,
    ) -> Self {
        Self {
            locale,
            local_server_config,
            require_earth,
            demo,
            config_file_settings,
            values,
        }
    }

    pub(crate) fn startup_projection(
        &self,
        key: &str,
    ) -> Option<minimium_commands::results::property::StartupValue<String>> {
        self.values.startup_projection(key)
    }
    pub(crate) fn requested(&self, key: &str) -> Option<&str> {
        self.values.requested(key)
    }
    pub(crate) fn effective(&self, key: &str) -> Option<&str> {
        self.values.effective(key)
    }
    pub(crate) fn source(&self, key: &str) -> PropertySource {
        self.values.source(key)
    }
    pub(crate) fn difference_reason(&self, key: &str) -> Option<&str> {
        self.values.difference_reason(key)
    }
    pub(crate) fn set(&mut self, value: ClientPropertyValue) {
        self.values.set(value);
    }
    pub(crate) fn set_effective(&mut self, value: ClientPropertyValue, reason: Option<String>) {
        self.values.set_effective(value, reason);
    }
    pub(crate) fn reset(&mut self, key: &str) -> Option<ClientPropertyValue> {
        self.values.reset(key)
    }
}

pub(crate) struct SettingsStore {
    fov_version: PropertyRevision,
    locale_revision: u64,
    startup: BTreeMap<String, SettingRecord>,
    records: BTreeMap<String, SettingRecord>,
}

#[derive(Clone, Debug, PartialEq)]
struct SettingValue {
    typed: ClientPropertyValue,
    encoded: String,
}

impl SettingValue {
    fn new(typed: ClientPropertyValue) -> Self {
        let encoded = typed.encode();
        Self { typed, encoded }
    }
    fn as_str(&self) -> &str {
        &self.encoded
    }
}

#[derive(Clone, Debug, PartialEq)]
struct SettingRecord {
    pub(crate) requested: SettingValue,
    pub(crate) effective: Option<SettingValue>,
    pub(crate) source: PropertySource,
    pub(crate) difference_reason: Option<String>,
}

impl SettingRecord {
    fn resolved(value: SettingValue, source: PropertySource) -> Self {
        Self {
            requested: value.clone(),
            effective: Some(value),
            source,
            difference_reason: None,
        }
    }
}

impl SettingsStore {
    pub(crate) fn startup_projection(
        &self,
        key: &str,
    ) -> Option<minimium_commands::results::property::StartupValue<String>> {
        use minimium_commands::results::property::{ConfigurationSource, StartupValue};
        let r = self.startup.get(key)?;
        Some(StartupValue {
            value: r.requested.encoded.clone(),
            source: ConfigurationSource::from_source(r.source)?,
        })
    }
    pub(crate) fn new(startup: &StartupSettings) -> Self {
        Self {
            locale_revision: 0,
            fov_version: PropertyRevision {
                base: SettingsBaseId::new(),
                revision: 0,
            },
            records: startup.records.clone(),
            startup: startup.records.clone(),
        }
    }

    pub(crate) fn requested(&self, key: &str) -> Option<&str> {
        self.records
            .get(key)
            .map(|record| record.requested.as_str())
    }

    pub(crate) fn effective(&self, key: &str) -> Option<&str> {
        self.records
            .get(key)
            .and_then(|record| record.effective.as_ref())
            .map(SettingValue::as_str)
    }

    pub(crate) fn difference_reason(&self, key: &str) -> Option<&str> {
        self.records
            .get(key)
            .and_then(|record| record.difference_reason.as_deref())
    }

    pub(crate) fn source(&self, key: &str) -> PropertySource {
        self.records
            .get(key)
            .map_or(PropertySource::Default, |record| record.source)
    }

    pub(crate) fn set(&mut self, value: ClientPropertyValue) {
        let key = value.key().key();
        let value = SettingValue::new(value);
        let record = self
            .records
            .entry(key.to_owned())
            .or_insert_with(|| SettingRecord::resolved(value.clone(), PropertySource::Default));
        record.requested = value.clone();
        record.effective = Some(value);
        record.source = PropertySource::Session;
        record.difference_reason = None;
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn set_effective(
        &mut self,
        value: ClientPropertyValue,
        difference_reason: Option<String>,
    ) {
        let key = value.key().key();
        let parsed = SettingValue::new(value);
        let record = self
            .records
            .entry(key.to_owned())
            .or_insert_with(|| SettingRecord::resolved(parsed.clone(), PropertySource::Default));
        record.effective = Some(parsed);
        record.difference_reason = difference_reason;
    }

    pub(crate) fn set_unavailable(&mut self, key: &str, reason: String) {
        if let Some(record) = self.records.get_mut(key) {
            record.effective = None;
            record.difference_reason = Some(reason);
        }
    }

    pub(crate) fn reset(&mut self, key: &str) -> Option<ClientPropertyValue> {
        let startup = self.startup.get(key)?;
        let value = startup.requested.typed.clone();
        self.records.insert(key.to_owned(), startup.clone());
        Some(value)
    }

    pub(crate) fn session_values(&self) -> BTreeMap<String, String> {
        self.records
            .iter()
            .filter(|(_, record)| record.source == PropertySource::Session)
            .map(|(key, record)| (key.clone(), record.requested.as_str().to_owned()))
            .collect()
    }
}
