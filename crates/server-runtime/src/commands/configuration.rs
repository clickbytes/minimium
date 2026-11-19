//! Resolved server configuration projected through the shared property model.

use minimium_commands::results::property::{
    ConfigurationSource, Freshness, ReadSource, StartupValue,
};
use minimium_commands::{PropertySource, PropertyValue};
use std::collections::BTreeMap;

/// The startup request and effective runtime value share the same provenance
/// contract as every other property. This alias adds no second state owner.
pub type ConfigurationValue<T> = PropertyValue<T>;

pub(crate) struct ConfigurationProjection<'a> {
    pub resolved: &'a BTreeMap<String, minimium_commands::ResolvedConfigValue>,
    pub catalog: &'a minimium_commands::operation::OperationCatalog,
    pub autosave_seconds: u64,
    pub autosave_session_override: bool,
    pub lanes: usize,
    pub sector_workers: usize,
    pub game_port: Option<u16>,
    pub material_http_port: u16,
}

pub(crate) fn project(
    context: ConfigurationProjection<'_>,
    key: &str,
) -> Option<ConfigurationValue<String>> {
    let (configured, default, configured_source) =
        crate::config::property_value(context.resolved, key)?;
    let (requested, effective, source, difference_reason) = match key {
        "server.autosave_interval" => {
            let value = context.autosave_seconds.to_string();
            (
                value.clone(),
                value,
                if context.autosave_session_override {
                    PropertySource::Session
                } else {
                    configured_source
                },
                (context.autosave_seconds == 0).then_some("periodic saves disabled".into()),
            )
        }
        "server.threads" => {
            let effective = context.lanes.to_string();
            (
                configured.into(),
                effective.clone(),
                configured_source,
                (configured != effective)
                    .then_some("auto resolved to the available worker lane count".into()),
            )
        }
        "server.sector_workers" => {
            let effective = context.sector_workers.to_string();
            (
                configured.into(),
                effective.clone(),
                configured_source,
                (configured != effective)
                    .then_some("auto resolved to the configured sector worker count".into()),
            )
        }
        "server.game_port" => {
            let effective = context
                .game_port
                .map_or_else(|| "unavailable".to_owned(), |port| port.to_string());
            (
                configured.into(),
                effective.clone(),
                configured_source,
                (configured != effective).then_some("port 0 selected an available port".into()),
            )
        }
        "server.material_http_port" => {
            let effective = context.material_http_port.to_string();
            (
                configured.into(),
                effective.clone(),
                configured_source,
                (configured != effective).then_some(
                    "material endpoint is disabled or port 0 selected an available port".into(),
                ),
            )
        }
        _ => (
            configured.into(),
            configured.into(),
            configured_source,
            None,
        ),
    };
    Some(PropertyValue {
        lifecycle: context
            .catalog
            .resolve(key)
            .and_then(|entry| entry.property)?
            .lifecycle,
        startup: ConfigurationSource::from_source(configured_source).map(|source| StartupValue {
            value: configured.into(),
            source,
        }),
        session_override: (source == PropertySource::Session).then(|| requested.clone()),
        read_source: ReadSource::OwnerSnapshot,
        revision: None,
        freshness: Freshness::Current,
        requested: Some(requested),
        effective: Some(effective),
        default: default.map(str::to_owned),
        source,
        difference_reason,
    })
}

pub(crate) fn encode(key: &str, value: String) -> minimium_commands::operation::CommandValue {
    use minimium_commands::operation::{CommandValue as V, FiniteValue};
    match key {
        "server.threads" | "server.sector_workers" if value == "auto" => V::Enum(value),
        "server.threads"
        | "server.sector_workers"
        | "server.fluid.world_cells"
        | "server.fluid.server_cells"
        | "server.game_port"
        | "server.max_players"
        | "server.autosave_interval"
        | "server.material_http_port" => value.parse().map_or(V::Text(value), V::U64),
        "server.headless" | "server.dev-mode" => value.parse().map_or(V::Text(value), V::Bool),
        "world.climate.relief_influence" if value == "unset" => V::Enum(value),
        "world.climate.relief_influence" => value
            .parse::<f64>()
            .ok()
            .and_then(FiniteValue::new)
            .map_or(V::Text(value), V::Finite),
        _ => V::Text(value),
    }
}
