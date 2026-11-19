//! Primary manifest generation properties, independent of actor membership.

use minimium_commands::operation::{CommandValue as V, OperationCatalog};
use minimium_commands::results::property::{
    ConfigurationSource, Freshness, ReadSource, StartupValue,
};
use minimium_commands::{PropertySource, PropertyValue, ResolvedConfigValue};
use minimium_world::world::WorldManifest;
use std::collections::BTreeMap;

pub(crate) fn project(
    key: &str,
    manifest: WorldManifest,
    resolved: &BTreeMap<String, ResolvedConfigValue>,
    catalog: &OperationCatalog,
) -> Option<PropertyValue<String>> {
    let actual = match key {
        "world.seed" => manifest.generation().seed().get().to_string(),
        "world.generation_revision" => manifest.generation().revision().get().to_string(),
        "world.generator_digest" => manifest
            .generator_digest()
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        _ => return None,
    };
    let (requested, default, request_source) = if key == "world.seed" {
        crate::config::property_value(resolved, key)
            .map(|(requested, default, source)| {
                (requested.to_owned(), default.map(str::to_owned), source)
            })
            .unwrap_or_else(|| ("auto".into(), Some("auto".into()), PropertySource::Default))
    } else {
        (actual.clone(), None, PropertySource::Runtime)
    };
    Some(PropertyValue {
        lifecycle: catalog.resolve(key)?.property?.lifecycle,
        startup: ConfigurationSource::from_source(request_source).map(|source| StartupValue {
            value: requested.clone(),
            source,
        }),
        session_override: None,
        read_source: ReadSource::OwnerSnapshot,
        revision: None,
        freshness: Freshness::Current,
        requested: Some(requested),
        effective: Some(actual),
        default,
        source: PropertySource::Runtime,
        difference_reason: Some(format!(
            "actual primary world manifest; requested source={}",
            request_source.as_str()
        )),
    })
}

pub(crate) fn encode(key: &str, text: String) -> V {
    match key {
        "world.seed" if text == "auto" => V::Enum(text),
        "world.seed" => {
            let parsed = text
                .strip_prefix("0x")
                .map_or_else(|| text.parse::<u128>(), |hex| u128::from_str_radix(hex, 16));
            parsed.map_or(V::Text(text), V::U128)
        }
        "world.generation_revision" => text.parse::<u64>().map_or(V::Text(text), V::U64),
        "world.generator_digest" => V::Text(text),
        _ => V::Text(text),
    }
}
