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
    for property in PRESENTATION_PROPERTIES.iter() {
        package.property(resolve(property.key, false)?, property)?;
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
    package.alias(
        "player.chat.send",
        "say",
        "say TEXT",
        super::command_specs()
            .iter()
            .find(|spec| spec.name == "social say")
            .expect("registered chat command")
            .arguments,
    )?;
    package.alias("client.quit", "quit", "quit", &[])?;
    package.operation(resolve("client.camera.status", false)?)?;
    package.operation(resolve("client.render.water.status", false)?)?;
    package.operation(resolve("world.time.initial_config", false)?)?;
    package.operation(minimium_commands::syntax::bind::explain_entry())?;
    package.finish()
}

/// Session-only properties whose state is owned by the desktop presentation.
static PRESENTATION_PROPERTIES: [minimium_commands::PropertySpec; 3] = [
    minimium_commands::PropertySpec::new(
        "client.sky.meteors.debug.mode",
        minimium_commands::PropertyType::Enumeration,
        None,
        "Local meteor diagnostics mode.",
        Some("off"),
        Some("off|light|occlusion|paths|budget"),
        "DesktopHost",
        minimium_commands::PropertyPermission::Local,
        minimium_commands::PropertyLifecycle::Live,
        false,
        true,
    ),
    minimium_commands::PropertySpec::new(
        "client.sky.meteors.debug.light",
        minimium_commands::PropertyType::Enumeration,
        None,
        "Local meteor light diagnostics mode.",
        Some("on"),
        Some("on|off|transmission"),
        "DesktopHost",
        minimium_commands::PropertyPermission::Local,
        minimium_commands::PropertyLifecycle::Live,
        false,
        true,
    ),
    minimium_commands::PropertySpec::new(
        "client.render.bolids.transmission",
        minimium_commands::PropertyType::Boolean,
        None,
        "Local bolid transmission diagnostic.",
        Some("false"),
        Some("true|false"),
        "DesktopHost",
        minimium_commands::PropertyPermission::Local,
        minimium_commands::PropertyLifecycle::Live,
        false,
        true,
    ),
];

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
