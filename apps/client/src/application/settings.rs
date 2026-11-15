use minimium_commands::PropertySource;
use std::collections::BTreeMap;

#[cfg(feature = "desktop")]
pub(crate) struct ClientSettings {
    pub(crate) locale: minimium_localization::Locale,
    pub(crate) local_server_config: minimium_client_runtime::LocalServerLaunchConfig,
    pub(crate) require_earth: bool,
    pub(crate) demo: bool,
    pub(crate) config_file_settings: BTreeMap<String, String>,
    values: SettingsStore,
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
    pub(crate) fn set(&mut self, key: &str, value: String) {
        self.values.set(key, value);
    }
    pub(crate) fn set_effective(&mut self, key: &str, value: String, reason: Option<String>) {
        self.values.set_effective(key, value, reason);
    }
    pub(crate) fn reset(&mut self, key: &str) {
        self.values.reset(key);
    }
    pub(crate) fn session_values(&self) -> BTreeMap<String, String> {
        self.values.session_values()
    }
}

pub(crate) struct SettingsStore {
    startup: BTreeMap<String, SettingRecord>,
    records: BTreeMap<String, SettingRecord>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SettingValue {
    Locale(minimium_localization::Locale),
    InputBindings(ValidatedInputBindings),
    Text(String),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ValidatedInputBindings(String);

impl ValidatedInputBindings {
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        crate::input::InputMapper::from_setting(value)?;
        Ok(Self(value.to_owned()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl SettingValue {
    fn parse(key: &str, value: &str) -> Self {
        match key {
            "ui.locale" => Self::Locale(
                minimium_localization::Locale::from_setting(value)
                    .expect("locale values are validated before entering settings"),
            ),
            "client.input.bindings" => Self::InputBindings(
                ValidatedInputBindings::parse(value)
                    .expect("binding values are validated before entering settings"),
            ),
            _ => Self::Text(value.to_owned()),
        }
    }

    fn as_str(&self) -> &str {
        match self {
            Self::Locale(minimium_localization::Locale::English) => "en",
            Self::Locale(minimium_localization::Locale::Polish) => "pl",
            Self::InputBindings(value) => value.as_str(),
            Self::Text(value) => value,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SettingRecord {
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
    pub(crate) fn new(
        startup: BTreeMap<String, String>,
        sources: BTreeMap<String, PropertySource>,
    ) -> Self {
        let startup = startup
            .into_iter()
            .map(|(key, value)| {
                let source = sources
                    .get(&key)
                    .copied()
                    .unwrap_or(PropertySource::Default);
                (
                    key.clone(),
                    SettingRecord::resolved(SettingValue::parse(&key, &value), source),
                )
            })
            .collect::<BTreeMap<_, _>>();
        Self {
            records: startup.clone(),
            startup,
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

    pub(crate) fn set(&mut self, key: &str, value: String) {
        let value = SettingValue::parse(key, &value);
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
        key: &str,
        value: String,
        difference_reason: Option<String>,
    ) {
        let parsed = SettingValue::parse(key, &value);
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

    pub(crate) fn reset(&mut self, key: &str) {
        if let Some(startup) = self.startup.get(key) {
            self.records.insert(key.to_owned(), startup.clone());
        }
    }

    pub(crate) fn session_values(&self) -> BTreeMap<String, String> {
        self.records
            .iter()
            .filter(|(_, record)| record.source == PropertySource::Session)
            .map(|(key, record)| (key.clone(), record.requested.as_str().to_owned()))
            .collect()
    }
}
