use minimium_gameplay::{
    CommandDefinition, CommandScope, CommandViewRequest, EngineModule, GameplaySystem,
    ModuleCommandContext, ModuleCommandErrorCode, ModuleCommandHandler, ModuleCommandResult,
    ModuleDescriptor, ModuleError, ModuleRegistrar, ModuleRegistry, WorldEffect,
};

const USAGE: &str = "sample inspect-and-remove";

fn main() -> Result<(), ModuleError> {
    let mut registry = ModuleRegistry::<ModuleCommandHandler, GameplaySystem>::default();
    registry.install(&NeutralCommandModule)
}

/// Example game module using only `minimium-gameplay` command contracts.
pub struct NeutralCommandModule;

impl EngineModule<ModuleCommandHandler, GameplaySystem> for NeutralCommandModule {
    fn descriptor(&self) -> ModuleDescriptor {
        ModuleDescriptor {
            id: "neutral-example",
            version: 1,
            requires: &[],
        }
    }

    fn register(
        &self,
        registrar: &mut ModuleRegistrar<'_, ModuleCommandHandler, GameplaySystem>,
    ) -> Result<(), ModuleError> {
        registrar.command(CommandDefinition {
            name: USAGE,
            aliases: &[],
            usage: USAGE,
            arguments: &[],
            scope: CommandScope::ServerAdmin,
            handler: inspect_and_remove,
        })
    }
}

fn inspect_and_remove(
    context: &mut ModuleCommandContext<'_>,
    _arguments: &[&str],
) -> ModuleCommandResult {
    let request = CommandViewRequest::new(256, 1024 * 1024).expect("valid example page limits");
    let Some(world) = context
        .worlds(&request)
        .ok()
        .and_then(|page| page.rows.into_iter().next())
    else {
        return ModuleCommandResult::success("No worlds are available.");
    };
    let entities = match context.entities(world.id, &request) {
        Ok(entities) => entities,
        Err(_) => {
            let code = ModuleCommandErrorCode::new("example.world_read_failed")
                .expect("static error code is valid");
            return ModuleCommandResult::error(code, "The selected world could not be read.");
        }
    };
    let Some(entity) = entities.rows.into_iter().next() else {
        return ModuleCommandResult::success("The selected world has no entities.");
    };

    match context.emit(WorldEffect::DespawnEntity {
        world: world.id,
        entity,
    }) {
        Ok(()) => ModuleCommandResult::success("Entity removal was requested."),
        Err(_) => {
            let code = ModuleCommandErrorCode::new("example.effect_buffer_refused")
                .expect("static error code is valid");
            ModuleCommandResult::error(code, "The effect buffer refused the request.")
        }
    }
}
