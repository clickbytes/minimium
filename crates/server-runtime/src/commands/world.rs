//! Example extension: owns the registration, validation and execution of its commands.
use super::{CommandContext, CommandPayload, CommandReply, ModuleHandler};
use crate::modules::{
    CommandDefinition, CommandScope, EngineModule, ModuleDescriptor, ModuleError, ModuleRegistrar,
    Stage, SystemDefinition,
};

const WORLD_INSPECT_ARGUMENTS: &[minimium_commands::CommandArgumentSpec] =
    &[minimium_commands::CommandArgumentSpec::new(
        "id",
        minimium_commands::CommandArgumentType::Integer,
        false,
        false,
        None,
        Some("positive decimal integer"),
        None,
        "world instance identifier",
    )];
use crate::runtime::schedule::SystemFn;
use minimium_gameplay::GameplaySystemContext;

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
            arguments: &[],
            scope: CommandScope::ServerAdmin,
            handler: world_list,
        })?;
        registry.command(CommandDefinition {
            name: "world inspect",
            aliases: &[],
            usage: "world inspect ID",
            arguments: WORLD_INSPECT_ARGUMENTS,
            scope: CommandScope::ServerAdmin,
            handler: world_inspect,
        })?;
        registry.command(CommandDefinition {
            name: "entity list",
            aliases: &[],
            usage: "entity list",
            arguments: &[],
            scope: CommandScope::ServerAdmin,
            handler: entity_list,
        })?;
        registry.system(SystemDefinition {
            id: "world-instance-count",
            stage: Stage::Startup,
            before: &[],
            after: &[],
            handler: record_instance_count,
        })
    }
}

fn record_instance_count(context: &mut dyn GameplaySystemContext) {
    debug_assert_eq!(context.stage(), Stage::Startup);
    let world_count = context.world_count();
    context.record_startup_run(world_count);
}

fn invalid(usage: &'static str) -> CommandReply {
    CommandReply::error("server.command.invalid_arguments", usage)
}

fn world_list(context: &mut CommandContext<'_>, args: &[&str]) -> CommandReply {
    if !args.is_empty() {
        return invalid("usage: world list");
    }
    let total = context.worlds.iter().count();
    let rows = context
        .worlds
        .iter()
        .take(256)
        .map(|(id, instance)| super::WorldRow {
            id,
            players: instance.world.player_count(),
            topology: instance.manifest.topology(),
            generation: instance.manifest.generation().revision(),
        })
        .collect::<Vec<_>>();
    CommandReply::success(CommandPayload::WorldList {
        rows,
        truncated: total > 256,
    })
}

fn world_inspect(context: &mut CommandContext<'_>, args: &[&str]) -> CommandReply {
    let [raw] = args else {
        return invalid("usage: world inspect ID");
    };
    let Ok(value) = raw.parse::<u128>() else {
        return invalid("world ID must be a positive decimal integer");
    };
    let Some(id) = minimium_world::world::WorldInstanceId::new(value) else {
        return invalid("world ID must be nonzero");
    };
    let Some(instance) = context.worlds.get(id) else {
        return invalid("world instance not found");
    };
    CommandReply::success(CommandPayload::WorldInspect(super::WorldInspect {
        id,
        players: instance.world.player_count(),
        manifest: instance.manifest,
    }))
}

fn entity_list(context: &mut CommandContext<'_>, args: &[&str]) -> CommandReply {
    if !args.is_empty() {
        return invalid("usage: entity list");
    }
    let mut rows: Vec<_> = context
        .worlds
        .iter()
        .flat_map(|(world_id, instance)| {
            instance.entities.iter().map(move |id| super::EntityRow {
                world: world_id,
                kind: super::EntityKind::Entity(id.get()),
            })
        })
        .take(256)
        .collect();
    let entity_count: usize = context
        .worlds
        .iter()
        .map(|(_, instance)| instance.entities.iter().count())
        .sum();
    let player_count = context.worlds.total_players();
    // Legacy players are not yet EntityIds; display them separately until migrated.
    let remaining = 256usize.saturating_sub(rows.len());
    rows.extend(
        context
            .worlds
            .iter()
            .flat_map(|(world_id, instance)| {
                instance
                    .world
                    .snapshot()
                    .into_iter()
                    .map(move |player| super::EntityRow {
                        world: world_id,
                        kind: super::EntityKind::Player(player.id, player.name.to_string()),
                    })
            })
            .take(remaining),
    );
    let truncated = entity_count.saturating_add(player_count) > rows.len();
    CommandReply::success(CommandPayload::EntityList { rows, truncated })
}
