//! Example extension: owns the registration, validation and execution of its commands.
use super::{CommandContext, CommandReply, ModuleHandler};
use crate::runtime::schedule::{ServerSystemContext, SystemFn};
use minimium_platform::modules::{
    CommandDefinition, CommandScope, EngineModule, ModuleDescriptor, ModuleError, ModuleRegistrar,
    Stage, SystemDefinition,
};

pub(super) struct WorldCommands;

impl EngineModule<ModuleHandler, SystemFn> for WorldCommands {
    fn descriptor(&self) -> ModuleDescriptor {
        ModuleDescriptor {
            id: "world",
            version: 1,
            requires: &[],
        }
    }

    fn register(
        &self,
        registry: &mut ModuleRegistrar<'_, ModuleHandler, SystemFn>,
    ) -> Result<(), ModuleError> {
        registry.command(CommandDefinition {
            name: "world list",
            aliases: &[],
            usage: "world list",
            scope: CommandScope::ServerAdmin,
            handler: world_list,
        })?;
        registry.command(CommandDefinition {
            name: "world inspect",
            aliases: &[],
            usage: "world inspect ID",
            scope: CommandScope::ServerAdmin,
            handler: world_inspect,
        })?;
        registry.command(CommandDefinition {
            name: "entity list",
            aliases: &[],
            usage: "entity list",
            scope: CommandScope::ServerAdmin,
            handler: entity_list,
        })?;
        registry.system(SystemDefinition {
            id: "world.instance-count",
            stage: Stage::Startup,
            handler: record_instance_count,
        })
    }
}

fn record_instance_count(context: &mut ServerSystemContext<'_>) {
    debug_assert_eq!(context.stage, Stage::Startup);
    context.diagnostics.world_instance_count = context.worlds.iter().count();
    context.diagnostics.startup_system_runs =
        context.diagnostics.startup_system_runs.saturating_add(1);
}

fn success(text: String) -> CommandReply {
    CommandReply {
        success: true,
        text,
        stop: false,
    }
}

fn invalid(usage: &'static str) -> CommandReply {
    CommandReply {
        success: false,
        text: usage.into(),
        stop: false,
    }
}

fn world_list(context: &mut CommandContext<'_>, args: &[&str]) -> CommandReply {
    if !args.is_empty() {
        return invalid("usage: world list");
    }
    let rows = context
        .worlds
        .iter()
        .map(|(id, instance)| {
            format!(
                "{id}: players={} topology={:?} generation={:?}",
                instance.world.player_count(),
                instance.manifest.topology(),
                instance.manifest.generation().revision()
            )
        })
        .collect::<Vec<_>>();
    success(rows.join("; "))
}

fn world_inspect(context: &mut CommandContext<'_>, args: &[&str]) -> CommandReply {
    let [raw] = args else {
        return invalid("usage: world inspect ID");
    };
    let Ok(value) = raw.parse::<u128>() else {
        return invalid("world ID must be a positive decimal integer");
    };
    let Some(id) = minimium_core::world::WorldInstanceId::new(value) else {
        return invalid("world ID must be nonzero");
    };
    let Some(instance) = context.worlds.get(id) else {
        return invalid("world instance not found");
    };
    success(format!(
        "{id}: players={} manifest={:?}",
        instance.world.player_count(),
        instance.manifest
    ))
}

fn entity_list(context: &mut CommandContext<'_>, args: &[&str]) -> CommandReply {
    if !args.is_empty() {
        return invalid("usage: entity list");
    }
    let mut rows: Vec<_> = context
        .worlds
        .iter()
        .flat_map(|(world_id, instance)| {
            instance
                .entities
                .iter()
                .map(move |id| format!("world={world_id} entity={}", id.get()))
        })
        .collect();
    // Legacy players are not yet EntityIds; display them separately until migrated.
    rows.extend(context.worlds.iter().flat_map(|(world_id, instance)| {
        instance
            .world
            .snapshot()
            .into_iter()
            .map(move |player| format!("world={world_id} player={}:{}", player.id, player.name))
    }));
    if rows.is_empty() {
        success("no runtime entities or players".into())
    } else {
        success(rows.join("; "))
    }
}
