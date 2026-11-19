use crate::commands::{CommandHelp, ModuleHandler};
use crate::modules::ModuleRegistry;
use crate::runtime::schedule::SystemFn;
use minimium_commands::operation::*;

mod admin;
mod bolids;
use minimium_environment::console::{clock_entry, ClockReadBinding};
mod config;
mod console;
mod persistence;
mod primary;
mod worlds;

pub(crate) fn build(
    registry: &ModuleRegistry<ModuleHandler, SystemFn>,
) -> Result<OperationCatalog, CatalogError> {
    let mut package = LegacyPackage::default();
    for property in super::server_properties() {
        package.property(resolve(property.key, property.persistent)?, property)?;
    }
    for spec in super::server_registry().commands() {
        // The legacy time grouping introduces a namespace. Its subcommands carry the bindings.
        if spec.name != "time" {
            package.command(
                resolve(spec.name, false)?,
                spec.name,
                spec.alias,
                spec.usage,
                spec.arguments,
            )?;
        }
    }
    for cmd in registry.commands() {
        let def = &cmd.definition;
        let mut entry = if cmd.module == "world" {
            resolve(def.name, false)?
        } else {
            let path = format!("module.{}.{}", cmd.module, def.name.replace(' ', "."));
            DomainBinding {
                owner: cmd.module,
                scope: "Server",
                context: "installed module",
                permission: match def.scope {
                    crate::modules::CommandScope::ServerAdmin => "ServerAdmin",
                    crate::modules::CommandScope::ClientLocal => "ClientLocal",
                    crate::modules::CommandScope::PlayerAction => "PlayerAction",
                },
                capability: None,
                source: "OwnerSnapshot",
                persistence: PersistenceKind::None,
            }
            .declaration(
                &path,
                OperationKind::Method,
                &[],
                TypeRef("ModuleCommandResult"),
                None,
                Effect::LegacyModule,
            )
        };
        if cmd.module != "world" {
            entry.spec.arguments = def.arguments.iter().map(ParameterSpec::from).collect();
        }
        package.command(
            entry,
            def.name,
            def.aliases.iter().copied(),
            def.usage,
            def.arguments,
        )?;
    }
    package.operation(minimium_commands::syntax::bind::explain_entry())?;
    package.finish()
}

pub(crate) fn command_help(catalog: &OperationCatalog, target: &str) -> Option<CommandHelp> {
    if target == "time" || target == "world.time" {
        let children = catalog
            .children("world.time")
            .into_iter()
            .collect::<Vec<_>>()
            .join(", ");
        return Some(CommandHelp {
            usage: minimium_environment::TIME_USAGE,
            arguments: &[],
            description: format!("world.time: {children}"),
        });
    }
    let entry = catalog.resolve(target)?;
    let legacy = entry
        .aliases
        .iter()
        .find(|a| a.name == target)
        .or_else(|| entry.aliases.first());
    Some(CommandHelp {
        usage: legacy
            .map(|a| a.usage)
            .or_else(|| entry.property.map(|p| p.key))
            .unwrap_or("help OPERATION"),
        arguments: legacy.map_or(&[], |a| a.arguments),
        description: entry.describe(),
    })
}

pub(crate) fn builtin_usages(catalog: &OperationCatalog) -> Vec<&'static str> {
    super::server_registry()
        .commands()
        .iter()
        .filter_map(|spec| {
            catalog
                .resolve(spec.name)?
                .aliases
                .iter()
                .find(|alias| alias.name == spec.name)
                .map(|alias| alias.usage)
        })
        .collect()
}

fn resolve(name: &str, _persistent: bool) -> Result<CatalogEntry, CatalogError> {
    config::entry(name)
        .or_else(|| primary::entry(name))
        .or_else(|| clock_entry(name, ClockReadBinding::OwnerSnapshot))
        .or_else(|| bolids::entry(name))
        .or_else(|| console::entry(name))
        .or_else(|| admin::entry(name))
        .or_else(|| persistence::entry(name))
        .or_else(|| worlds::entry(name))
        .ok_or_else(|| CatalogError::InvalidName(name.to_owned()))
}
