//! Server-owned world commands exercise the same bounded module command port as extensions.
use super::{CommandPayload, CommandReply, ModuleHandler};
use crate::modules::{
    CommandDefinition, CommandScope, EngineModule, ModuleDescriptor, ModuleError, ModuleRegistrar,
    Stage, SystemDefinition,
};
use crate::world_registry::WorldRegistry;
use minimium_gameplay::{
    CommandPlayerView, CommandViewCursorIssuer, CommandViewError, CommandViewKind, CommandViewPage,
    CommandViewRequest, CommandViewScope, CommandWorldView, ComponentView, ModuleCommandContext,
};
use minimium_world::{entity::EntityId, world::WorldInstanceId};

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
const WORLD_LIST_ARGUMENTS: &[minimium_commands::CommandArgumentSpec] =
    &[minimium_commands::CommandArgumentSpec::new(
        "cursor",
        minimium_commands::CommandArgumentType::Text,
        true,
        false,
        None,
        None,
        None,
        "opaque continuation cursor",
    )];
const ENTITY_LIST_ARGUMENTS: &[minimium_commands::CommandArgumentSpec] = WORLD_LIST_ARGUMENTS;
const ENTITY_ARGUMENTS: &[minimium_commands::CommandArgumentSpec] = &[
    minimium_commands::CommandArgumentSpec::new(
        "world",
        minimium_commands::CommandArgumentType::Integer,
        false,
        false,
        None,
        Some("positive decimal integer"),
        None,
        "world instance identifier",
    ),
    minimium_commands::CommandArgumentSpec::new(
        "entity",
        minimium_commands::CommandArgumentType::Integer,
        false,
        false,
        None,
        Some("positive decimal integer"),
        None,
        "entity identifier",
    ),
];
const ENTITY_INSPECT_ARGUMENTS: &[minimium_commands::CommandArgumentSpec] = &[
    ENTITY_ARGUMENTS[0],
    ENTITY_ARGUMENTS[1],
    minimium_commands::CommandArgumentSpec::new(
        "cursor",
        minimium_commands::CommandArgumentType::Text,
        true,
        false,
        None,
        None,
        None,
        "opaque continuation cursor",
    ),
];
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
            usage: "world list [CURSOR]",
            arguments: WORLD_LIST_ARGUMENTS,
            scope: CommandScope::ServerAdmin,
            handler: ModuleHandler::Builtin(world_list),
        })?;
        registry.command(CommandDefinition {
            name: "world inspect",
            aliases: &[],
            usage: "world inspect ID",
            arguments: WORLD_INSPECT_ARGUMENTS,
            scope: CommandScope::ServerAdmin,
            handler: ModuleHandler::Builtin(world_inspect),
        })?;
        registry.command(CommandDefinition {
            name: "entity list",
            aliases: &[],
            usage: "entity list [CURSOR]",
            arguments: ENTITY_LIST_ARGUMENTS,
            scope: CommandScope::ServerAdmin,
            handler: ModuleHandler::Builtin(entity_list),
        })?;
        registry.command(CommandDefinition {
            name: "entity inspect",
            aliases: &[],
            usage: "entity inspect WORLD ENTITY [CURSOR]",
            arguments: ENTITY_INSPECT_ARGUMENTS,
            scope: CommandScope::ServerAdmin,
            handler: ModuleHandler::Builtin(entity_inspect),
        })?;
        registry.command(CommandDefinition {
            name: "entity despawn",
            aliases: &[],
            usage: "entity despawn WORLD ENTITY",
            arguments: ENTITY_ARGUMENTS,
            scope: CommandScope::ServerAdmin,
            handler: ModuleHandler::Builtin(entity_despawn),
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

fn first_read_request() -> CommandViewRequest {
    CommandViewRequest::new(
        minimium_gameplay::MAX_COMMAND_VIEW_ROWS,
        minimium_gameplay::MAX_COMMAND_VIEW_BYTES,
    )
    .expect("built-in command page limits are valid")
}

fn world_list(context: &mut ModuleCommandContext<'_>, args: &[&str]) -> CommandReply {
    let request = match access::page(
        context,
        args,
        1,
        "usage: world list [CURSOR] or world.instances.list(cursor?, limit?)",
    ) {
        Ok(request) => request,
        Err(reply) => return reply,
    };
    let page = match context.worlds(&request) {
        Ok(page) => page,
        Err(CommandViewError::StaleCursor) => {
            return CommandReply::error("server.command.stale_cursor", "world cursor is stale")
        }
        Err(_) => {
            return CommandReply::error("server.command.read_failed", "world page unavailable")
        }
    };
    let truncated = page.truncated();
    let next_cursor = page.next.as_ref().map(|cursor| cursor.token());
    let rows = page
        .rows
        .into_iter()
        .map(|world| super::WorldRow {
            id: world.id,
            players: world.players,
            topology: world.topology,
            generation: world.generation,
        })
        .collect::<Vec<_>>();
    CommandReply::success(CommandPayload::WorldList {
        rows,
        truncated,
        next_cursor,
    })
}

fn world_inspect(context: &mut ModuleCommandContext<'_>, args: &[&str]) -> CommandReply {
    let [raw] = args else {
        return invalid("usage: world inspect ID");
    };
    let Ok(value) = raw.parse::<u128>() else {
        return invalid("world ID must be a positive decimal integer");
    };
    let Some(id) = minimium_world::world::WorldInstanceId::new(value) else {
        return invalid("world ID must be nonzero");
    };
    let world = match context.world(id) {
        Ok(world) => world,
        Err(CommandViewError::UnknownWorld) => return invalid("world instance not found"),
        Err(CommandViewError::UnknownEntity) => unreachable!("world lookup cannot miss an entity"),
        Err(_) => return CommandReply::error("server.command.read_failed", "world lookup failed"),
    };
    CommandReply::success(CommandPayload::WorldInspect(super::WorldInspect {
        id,
        players: world.players,
        manifest: world.manifest,
    }))
}

fn entity_list(context: &mut ModuleCommandContext<'_>, args: &[&str]) -> CommandReply {
    let request = match access::page(
        context,
        args,
        1,
        "usage: entity list [CURSOR] or world.objects.list(cursor?, limit?)",
    ) {
        Ok(request) => request,
        Err(reply) => return reply,
    };
    let page = match context.entity_rows(&request) {
        Ok(page) => page,
        Err(CommandViewError::StaleCursor) => {
            return CommandReply::error("server.command.stale_cursor", "object cursor is stale")
        }
        Err(CommandViewError::UnknownWorld) => return invalid("world instance not found"),
        Err(_) => {
            return CommandReply::error("server.command.read_failed", "entity page unavailable")
        }
    };
    let next_cursor = page.next.as_ref().map(|cursor| cursor.token());
    let rows = page
        .rows
        .into_iter()
        .map(|row| super::EntityRow {
            world: row.world,
            kind: match row.kind {
                minimium_gameplay::CommandEntityKind::Entity(entity) => {
                    super::EntityKind::Entity(entity.get())
                }
                minimium_gameplay::CommandEntityKind::Player(id, name) => {
                    super::EntityKind::Player(id, name)
                }
            },
        })
        .collect();
    let truncated = next_cursor.is_some();
    CommandReply::success(CommandPayload::EntityList {
        rows,
        truncated,
        next_cursor,
    })
}

fn entity_arguments(
    args: &[&str],
    usage: &'static str,
) -> Result<(WorldInstanceId, EntityId), CommandReply> {
    let [raw_world, raw_entity] = args else {
        return Err(invalid(usage));
    };
    let world = raw_world
        .parse::<u128>()
        .ok()
        .and_then(WorldInstanceId::new)
        .ok_or_else(|| invalid("world ID must be a positive decimal integer"))?;
    let entity = raw_entity
        .parse::<u64>()
        .ok()
        .and_then(EntityId::new)
        .ok_or_else(|| invalid("entity ID must be a positive decimal integer"))?;
    Ok((world, entity))
}

fn entity_inspect(context: &mut ModuleCommandContext<'_>, args: &[&str]) -> CommandReply {
    let (world, entity) = match entity_arguments(
        &args[..args.len().min(2)],
        "usage: entity inspect WORLD ENTITY [CURSOR]",
    ) {
        Ok(ids) => ids,
        Err(reply) => return reply,
    };
    if args.len() > 4 {
        return invalid("usage: entity inspect WORLD ENTITY [CURSOR]");
    }
    let request = match access::page(context, &args[2..], 1, "usage: entity inspect WORLD ENTITY [CURSOR] or world.entities.inspect(WORLD, ENTITY, cursor?, limit?)") {
        Ok(request) => request,
        Err(reply) => return reply,
    };
    let components = match context.component_metadata(world, entity, &request) {
        Ok(page) => page,
        Err(CommandViewError::StaleCursor) => {
            return CommandReply::error("server.command.stale_cursor", "component cursor is stale")
        }
        Err(CommandViewError::UnknownWorld) => return invalid("world instance not found"),
        Err(CommandViewError::UnknownEntity) => return invalid("entity not found in world"),
        Err(_) => {
            return CommandReply::error("server.command.read_failed", "component page unavailable")
        }
    };
    let truncated = components.truncated();
    let next_cursor = components.next.as_ref().map(|cursor| cursor.token());
    let rows = components
        .rows
        .into_iter()
        .map(|component| super::EntityComponentRow {
            kind: component.kind,
            schema: component.schema,
            bytes: component.bytes,
        })
        .collect();
    CommandReply::success(CommandPayload::EntityInspect {
        world,
        entity,
        rows,
        truncated,
        next_cursor,
    })
}

fn entity_despawn(context: &mut ModuleCommandContext<'_>, args: &[&str]) -> CommandReply {
    let (world, entity) = match entity_arguments(args, "usage: entity despawn WORLD ENTITY") {
        Ok(ids) => ids,
        Err(reply) => return reply,
    };
    if let Err(error) = context.components(world, entity, &first_read_request()) {
        return match error {
            CommandViewError::UnknownWorld => invalid("world instance not found"),
            CommandViewError::UnknownEntity => invalid("entity not found in world"),
            _ => CommandReply::error("server.command.read_failed", "component page unavailable"),
        };
    }
    if let Err(error) = context.emit(crate::modules::WorldEffect::DespawnEntity { world, entity }) {
        let (code, detail) = match error {
            crate::modules::EffectBufferError::OperationLimit => (
                "server.command.effect_limit",
                "command effect operation limit exceeded",
            ),
            crate::modules::EffectBufferError::PayloadLimit => (
                "server.command.effect_payload_limit",
                "command effect payload limit exceeded",
            ),
            crate::modules::EffectBufferError::ReceiptBackpressure => (
                "server.command.effect_receipt_backpressure",
                "command effect receipt capacity exhausted",
            ),
            crate::modules::EffectBufferError::IdentifierSpaceExhausted => (
                "server.command.effect_identifier_exhausted",
                "command effect identifier space exhausted",
            ),
        };
        return CommandReply::error(code, detail);
    }
    CommandReply::success(CommandPayload::EntityDespawn { world, entity })
}

mod access;
mod catalog;
mod read_view;
pub(crate) use catalog::entry as catalog_entry;
pub(crate) use read_view::RuntimeCommandReadView;
