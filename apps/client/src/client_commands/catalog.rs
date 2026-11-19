use minimium_commands::operation::{CatalogEntry, CatalogError, LegacyPackage, OperationCatalog};
use std::sync::OnceLock;

mod actor;
mod bolids;
use minimium_environment::console::{clock_entry, ClockReadBinding};
mod connection;
mod console;
mod movement;
mod presentation;
mod settings;

pub(super) fn catalog() -> &'static OperationCatalog {
    static CATALOG: OnceLock<OperationCatalog> = OnceLock::new();
    CATALOG.get_or_init(|| build().expect("invalid client operation declarations"))
}

fn build() -> Result<OperationCatalog, CatalogError> {
    let mut package = LegacyPackage::default();
    for property in super::properties::property_specs() {
        package.property(resolve(property.key, property.persistent)?, property)?;
    }
    package.alias(
        "client.sky.meteors.sound",
        "client.audio.meteors",
        "client.audio.meteors",
        &[],
    )?;
    for command in super::command_specs() {
        package.command(
            resolve(command.name, false)?,
            command.name,
            command.alias,
            command.usage,
            command.arguments,
        )?;
    }
    for &(name, usage) in super::presentation::COMMAND_FORMS {
        package.command(resolve(name, false)?, name, [], usage, &[])?;
    }
    package.finish()
}

fn resolve(name: &str, persistent: bool) -> Result<CatalogEntry, CatalogError> {
    presentation::entry(name, persistent)
        .or_else(|| settings::entry(name, persistent))
        .or_else(|| movement::entry(name))
        .or_else(|| clock_entry(name, ClockReadBinding::SessionSample))
        .or_else(|| console::entry(name))
        .or_else(|| connection::entry(name))
        .or_else(|| actor::entry(name))
        .or_else(|| bolids::entry(name))
        .ok_or_else(|| CatalogError::InvalidName(name.to_owned()))
}
