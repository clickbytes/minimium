use crate::PropertySource;
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fmt;

pub const MAX_CONFIG_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigErrorCode {
    TooLarge,
    InvalidDocument,
    UnsupportedSchema,
    UnknownKey,
    Serialization,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigError {
    pub code: ConfigErrorCode,
    pub key: Option<String>,
    pub reason: String,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(key) = &self.key {
            write!(formatter, "{} ({key})", self.reason)
        } else {
            formatter.write_str(&self.reason)
        }
    }
}

impl std::error::Error for ConfigError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigDocument {
    settings: BTreeMap<String, String>,
}

impl ConfigDocument {
    pub fn new(settings: BTreeMap<String, String>) -> Self {
        Self { settings }
    }

    pub fn settings(&self) -> &BTreeMap<String, String> {
        &self.settings
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, ConfigError> {
        if bytes.len() > MAX_CONFIG_BYTES {
            return Err(ConfigError {
                code: ConfigErrorCode::TooLarge,
                key: None,
                reason: format!("configuration exceeds {MAX_CONFIG_BYTES} bytes"),
            });
        }
        let source = std::str::from_utf8(bytes).map_err(|_| ConfigError {
            code: ConfigErrorCode::InvalidDocument,
            key: None,
            reason: "configuration must be UTF-8 RON".to_owned(),
        })?;
        let wire = ron::from_str::<ConfigDocumentWire>(source).map_err(|error| ConfigError {
            code: ConfigErrorCode::InvalidDocument,
            key: None,
            reason: format!("invalid configuration document: {error}"),
        })?;
        if wire.schema_version != 1 {
            return Err(ConfigError {
                code: ConfigErrorCode::UnsupportedSchema,
                key: None,
                reason: format!(
                    "unsupported configuration schema {}; expected 1",
                    wire.schema_version
                ),
            });
        }
        Ok(Self {
            settings: wire.settings.0,
        })
    }

    pub fn validate_keys<'a>(
        &self,
        known: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), ConfigError> {
        let known = known.into_iter().collect::<HashSet<_>>();
        if let Some(key) = self
            .settings
            .keys()
            .find(|key| !known.contains(key.as_str()))
        {
            return Err(ConfigError {
                code: ConfigErrorCode::UnknownKey,
                key: Some(key.clone()),
                reason: "unknown configuration key".to_owned(),
            });
        }
        Ok(())
    }

    pub fn to_ron(&self) -> Result<String, ConfigError> {
        ron::ser::to_string(&ConfigDocumentWireRef {
            schema_version: 1,
            settings: &self.settings,
        })
        .map_err(|error| ConfigError {
            code: ConfigErrorCode::Serialization,
            key: None,
            reason: format!("could not serialize configuration: {error}"),
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigDocumentWire {
    schema_version: u32,
    settings: UniqueSettings,
}

#[derive(Serialize)]
struct ConfigDocumentWireRef<'a> {
    schema_version: u32,
    settings: &'a BTreeMap<String, String>,
}

struct UniqueSettings(BTreeMap<String, String>);

impl<'de> Deserialize<'de> for UniqueSettings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SettingsVisitor;

        impl<'de> Visitor<'de> for SettingsVisitor {
            type Value = UniqueSettings;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a map of unique string setting keys to string values")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut values = BTreeMap::new();
                while let Some((key, value)) = map.next_entry::<String, String>()? {
                    if values.insert(key.clone(), value).is_some() {
                        return Err(serde::de::Error::custom(format!(
                            "duplicate setting key `{key}`"
                        )));
                    }
                }
                Ok(UniqueSettings(values))
            }
        }

        deserializer.deserialize_map(SettingsVisitor)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedConfigValue {
    pub requested: String,
    pub default: Option<String>,
    pub source: PropertySource,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ConfigResolution {
    values: BTreeMap<String, ResolvedConfigValue>,
}

impl ConfigResolution {
    pub fn resolve(
        defaults: &BTreeMap<String, String>,
        file: Option<&ConfigDocument>,
        environment: &BTreeMap<String, String>,
        command_line: &BTreeMap<String, String>,
    ) -> Self {
        let mut values = defaults
            .iter()
            .map(|(key, value)| {
                (
                    key.clone(),
                    ResolvedConfigValue {
                        requested: value.clone(),
                        default: Some(value.clone()),
                        source: PropertySource::Default,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        if let Some(file) = file {
            apply_layer(&mut values, &file.settings, PropertySource::File);
        }
        apply_layer(&mut values, environment, PropertySource::Environment);
        apply_layer(&mut values, command_line, PropertySource::CommandLine);
        Self { values }
    }

    pub fn get(&self, key: &str) -> Option<&ResolvedConfigValue> {
        self.values.get(key)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &ResolvedConfigValue)> {
        self.values.iter().map(|(key, value)| (key.as_str(), value))
    }
}

fn apply_layer(
    values: &mut BTreeMap<String, ResolvedConfigValue>,
    layer: &BTreeMap<String, String>,
    source: PropertySource,
) {
    for (key, value) in layer {
        let default = values
            .get(key)
            .and_then(|resolved| resolved.default.clone());
        values.insert(
            key.clone(),
            ResolvedConfigValue {
                requested: value.clone(),
                default,
                source,
            },
        );
    }
}
