//! Server-owned world commands exercise the same bounded module command port as extensions.
use super::{CommandPayload, CommandReply, ModuleHandler};
use crate::modules::{
    CommandDefinition, CommandScope, EngineModule, ModuleDescriptor, ModuleError, ModuleRegistrar,
    Stage, SystemDefinition,
};
use crate::world_registry::WorldRegistry;
use minimium_gameplay::{
    CommandPlayerView, CommandViewError, CommandViewPage, CommandWorldView, ComponentView,
    ModuleCommandContext, ModuleCommandReadView,
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
            usage: "entity list",
            arguments: &[],
            scope: CommandScope::ServerAdmin,
            handler: ModuleHandler::Builtin(entity_list),
        })?;
        registry.command(CommandDefinition {
            name: "entity inspect",
            aliases: &[],
            usage: "entity inspect WORLD ENTITY",
            arguments: ENTITY_ARGUMENTS,
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

fn world_list(context: &mut ModuleCommandContext<'_>, args: &[&str]) -> CommandReply {
    if !args.is_empty() {
        return invalid("usage: world list");
    }
    let page = context.worlds();
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
        truncated: page.truncated,
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
    };
    CommandReply::success(CommandPayload::WorldInspect(super::WorldInspect {
        id,
        players: world.players,
        manifest: world.manifest,
    }))
}

fn entity_list(context: &mut ModuleCommandContext<'_>, args: &[&str]) -> CommandReply {
    if !args.is_empty() {
        return invalid("usage: entity list");
    }
    let worlds = context.worlds();
    let total_worlds = context.world_count();
    let mut rows = Vec::new();
    let mut truncated = worlds.truncated;
    for (world_index, world) in worlds.rows.into_iter().enumerate() {
        let page = match context.entities(world.id) {
            Ok(page) => page,
            Err(CommandViewError::UnknownWorld) => return invalid("world instance not found"),
            Err(CommandViewError::UnknownEntity) => unreachable!("entity page has no entity ID"),
        };
        let page_truncated = page.truncated;
        for entity in page.rows {
            if rows.len() == 256 {
                truncated = true;
                break;
            }
            rows.push(super::EntityRow {
                world: world.id,
                kind: super::EntityKind::Entity(entity.get()),
            });
        }
        if rows.len() == 256 {
            truncated |= page_truncated || world_index + 1 < total_worlds;
            break;
        }
        truncated |= page_truncated;
    }
    let players = context.players();
    truncated |= players.truncated;
    for player in players.rows {
        if rows.len() == 256 {
            truncated = true;
            break;
        }
        if rows.len() < 256 {
            rows.push(super::EntityRow {
                world: player.world,
                kind: super::EntityKind::Player(player.id, player.name),
            });
        }
    }
    CommandReply::success(CommandPayload::EntityList { rows, truncated })
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
    let (world, entity) = match entity_arguments(args, "usage: entity inspect WORLD ENTITY") {
        Ok(ids) => ids,
        Err(reply) => return reply,
    };
    let components = match context.components(world, entity) {
        Ok(page) => page,
        Err(CommandViewError::UnknownWorld) => return invalid("world instance not found"),
        Err(CommandViewError::UnknownEntity) => return invalid("entity not found in world"),
    };
    let rows = components
        .rows
        .into_iter()
        .map(|component| super::EntityComponentRow {
            kind: component.kind,
            schema: component.schema,
            bytes: component.bytes.len(),
        })
        .collect();
    CommandReply::success(CommandPayload::EntityInspect {
        world,
        entity,
        rows,
        truncated: components.truncated,
    })
}

fn entity_despawn(context: &mut ModuleCommandContext<'_>, args: &[&str]) -> CommandReply {
    let (world, entity) = match entity_arguments(args, "usage: entity despawn WORLD ENTITY") {
        Ok(ids) => ids,
        Err(reply) => return reply,
    };
    if let Err(error) = context.components(world, entity) {
        return match error {
            CommandViewError::UnknownWorld => invalid("world instance not found"),
            CommandViewError::UnknownEntity => invalid("entity not found in world"),
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
        };
        return CommandReply::error(code, detail);
    }
    CommandReply::success(CommandPayload::EntityDespawn { world, entity })
}

pub(crate) struct RuntimeCommandReadView<'a> {
    worlds: &'a WorldRegistry,
}

impl<'a> RuntimeCommandReadView<'a> {
    pub(crate) fn new(worlds: &'a WorldRegistry) -> Self {
        Self { worlds }
    }

    fn world_view(&self, id: WorldInstanceId) -> Option<CommandWorldView> {
        let instance = self.worlds.get(id)?;
        Some(CommandWorldView {
            id,
            players: instance.world.player_count(),
            topology: instance.manifest.topology(),
            generation: instance.manifest.generation().revision(),
            manifest: instance.manifest,
        })
    }
}

impl ModuleCommandReadView for RuntimeCommandReadView<'_> {
    fn world_count(&self) -> usize {
        self.worlds.iter().count()
    }

    fn worlds(&self, max_rows: usize) -> CommandViewPage<CommandWorldView> {
        let mut rows = self
            .worlds
            .iter()
            .take(max_rows)
            .filter_map(|(id, _)| self.world_view(id))
            .collect::<Vec<_>>();
        let truncated = self.world_count() > rows.len();
        rows.shrink_to_fit();
        CommandViewPage::new(rows, truncated)
    }

    fn world(&self, id: WorldInstanceId) -> Result<CommandWorldView, CommandViewError> {
        self.world_view(id).ok_or(CommandViewError::UnknownWorld)
    }

    fn entities(
        &self,
        world: WorldInstanceId,
        max_rows: usize,
    ) -> Result<CommandViewPage<EntityId>, CommandViewError> {
        let instance = self
            .worlds
            .get(world)
            .ok_or(CommandViewError::UnknownWorld)?;
        let mut entities = instance.entities.iter();
        let rows = entities.by_ref().take(max_rows).collect::<Vec<_>>();
        let truncated = entities.next().is_some();
        Ok(CommandViewPage::new(rows, truncated))
    }

    fn players(&self, max_rows: usize, max_bytes: usize) -> CommandViewPage<CommandPlayerView> {
        // Active players are globally capped by `config::MAX_PLAYERS` (512).
        let mut players = self.worlds.player_snapshots().into_iter();
        let mut rows = Vec::new();
        let mut bytes = 0usize;
        let mut truncated = false;
        for (world, player) in players.by_ref() {
            let row_bytes = std::mem::size_of::<CommandPlayerView>() + player.name.len();
            if rows.len() >= max_rows || bytes.saturating_add(row_bytes) > max_bytes {
                truncated = true;
                break;
            }
            bytes += row_bytes;
            rows.push(CommandPlayerView {
                id: player.id,
                name: player.name.to_string(),
                world,
                position: player.position,
            });
        }
        truncated |= players.next().is_some();
        CommandViewPage::new(rows, truncated)
    }

    fn components(
        &self,
        world: WorldInstanceId,
        entity: EntityId,
        max_rows: usize,
        max_bytes: usize,
    ) -> Result<CommandViewPage<ComponentView<'_>>, CommandViewError> {
        let instance = self
            .worlds
            .get(world)
            .ok_or(CommandViewError::UnknownWorld)?;
        if !instance.entities.contains(entity) {
            return Err(CommandViewError::UnknownEntity);
        }
        let mut components = self
            .worlds
            .component_views(world)
            .ok_or(CommandViewError::UnknownWorld)?
            .filter(|(owner, _, _, _)| *owner == entity);
        let mut rows = Vec::new();
        let mut bytes = 0usize;
        let mut truncated = false;
        for (owner, kind, schema, payload) in components.by_ref() {
            let row_bytes = std::mem::size_of::<ComponentView<'_>>() + payload.len();
            if rows.len() >= max_rows || bytes.saturating_add(row_bytes) > max_bytes {
                truncated = true;
                break;
            }
            bytes += row_bytes;
            rows.push(ComponentView {
                entity: owner,
                kind,
                schema,
                bytes: payload,
            });
        }
        truncated |= components.next().is_some();
        Ok(CommandViewPage::new(rows, truncated))
    }
}
