use minimium_gameplay::{
    CommandViewRequest, EffectBufferError, EngineModule, GameplaySystem, GameplaySystemContext,
    ModuleCommandHandler, ModuleDescriptor, ModuleError, ModuleRegistrar, Stage, SystemDefinition,
    WorldEffect, WorldEffectOutcome,
};
use minimium_world::world::{PlanetFace, SectorKey, WorldTopology};

pub struct EffectFeedbackModule;

impl EngineModule<ModuleCommandHandler, GameplaySystem> for EffectFeedbackModule {
    fn descriptor(&self) -> ModuleDescriptor {
        ModuleDescriptor {
            id: "effect-feedback-example",
            version: 1,
            requires: &[],
        }
    }

    fn register(
        &self,
        registrar: &mut ModuleRegistrar<'_, ModuleCommandHandler, GameplaySystem>,
    ) -> Result<(), ModuleError> {
        registrar.system(SystemDefinition {
            id: "effect-feedback-startup",
            stage: Stage::Startup,
            before: &[],
            after: &[],
            handler: startup_feedback,
        })?;
        registrar.system(SystemDefinition {
            id: "effect-feedback-ordinary",
            stage: Stage::Environment,
            before: &[],
            after: &[],
            handler: ordinary_feedback,
        })
    }
}

fn first_page_request() -> Option<CommandViewRequest> {
    CommandViewRequest::new(256, 1024 * 1024).ok()
}

fn sector_for(topology: WorldTopology) -> SectorKey {
    match topology {
        WorldTopology::Dungeon => SectorKey::dungeon(0, 0, 0),
        WorldTopology::Planet => SectorKey::planet(PlanetFace::PositiveX, 0, 0)
            .expect("origin sector is valid on a planet face"),
    }
}

fn startup_feedback(context: &mut dyn GameplaySystemContext) {
    let Some(instance) = context.world_instance() else {
        return;
    };
    let Some(request) = first_page_request() else {
        return;
    };
    let Ok(page) = context.worlds(&request) else {
        return;
    };
    let Some(world) = page.rows.into_iter().find(|world| world.id == instance) else {
        return;
    };
    // Startup has no later system call; the host drains the resulting receipt after initialization.
    let _accepted_token = context.emit(WorldEffect::SpawnEntity {
        world: instance,
        sector: sector_for(world.topology),
        components: Vec::new(),
    });
}

fn ordinary_feedback(context: &mut dyn GameplaySystemContext) {
    for receipt in context.take_effect_receipts() {
        let _source_and_order = (receipt.token.producer(), receipt.token.sequence());
        match receipt.outcome {
            WorldEffectOutcome::EntitySpawned { world: _, entity } => {
                let _committed_entity_id = entity;
            }
            WorldEffectOutcome::EntityRejected(_) | WorldEffectOutcome::StageRejected => {
                let _rejection = receipt.outcome;
            }
            _ => {}
        }
    }

    if context.simulation_tick() != 1 {
        return;
    }
    let Some(request) = first_page_request() else {
        return;
    };
    let Ok(page) = context.worlds(&request) else {
        return;
    };
    let Some(world) = page.rows.into_iter().next() else {
        return;
    };
    let result: Result<_, EffectBufferError> = context.emit(WorldEffect::SpawnEntity {
        world: world.id,
        sector: sector_for(world.topology),
        components: Vec::new(),
    });
    let _accepted_token_is_not_a_commit_receipt = result;
}
