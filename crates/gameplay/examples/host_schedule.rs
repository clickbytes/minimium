//! A neutral host can validate its own small schedule without engine mechanics.
use minimium_gameplay::{
    EngineModule, GameplaySystem, GameplaySystemContext, ModuleDescriptor, ModuleError,
    ModuleRegistrar, ModuleRegistry, ScheduleDefinition, ScheduleNode, ScheduleNodeKind,
    ScheduleStep, Stage, SystemDefinition, FIXED_UPDATE_SCHEDULE_ANCHOR, STARTUP_SCHEDULE_ANCHOR,
};

struct ExampleModule;

fn inspect(_context: &mut dyn GameplaySystemContext) {}

impl EngineModule<(), GameplaySystem> for ExampleModule {
    fn descriptor(&self) -> ModuleDescriptor {
        ModuleDescriptor {
            id: "schedule example",
            version: 1,
            requires: &[],
        }
    }

    fn register(
        &self,
        registrar: &mut ModuleRegistrar<'_, (), GameplaySystem>,
    ) -> Result<(), ModuleError> {
        registrar.system(SystemDefinition {
            id: "example inspection",
            stage: Stage::FixedUpdate,
            before: &[FIXED_UPDATE_SCHEDULE_ANCHOR],
            after: &["example opening"],
            handler: inspect,
        })
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    const NODES: &[ScheduleNode] = &[
        ScheduleNode {
            id: "example opening",
            kind: ScheduleNodeKind::Operation,
            before: &[],
            after: &[],
        },
        ScheduleNode {
            id: FIXED_UPDATE_SCHEDULE_ANCHOR,
            kind: ScheduleNodeKind::StageBarrier(Stage::FixedUpdate),
            before: &[],
            after: &["example opening"],
        },
        ScheduleNode {
            id: "example closing",
            kind: ScheduleNodeKind::Operation,
            before: &[],
            after: &[FIXED_UPDATE_SCHEDULE_ANCHOR],
        },
    ];
    const REQUIRED: &[Stage] = &[Stage::Startup, Stage::FixedUpdate];
    let definition = ScheduleDefinition::new(NODES, STARTUP_SCHEDULE_ANCHOR, REQUIRED);
    let mut registry: ModuleRegistry<(), GameplaySystem> = ModuleRegistry::default();
    registry.install(&ExampleModule)?;
    let plan = registry.ordered_schedule(&definition)?;

    for step in plan.steps() {
        match step {
            ScheduleStep::Operation(id) => println!("operation: {id}"),
            ScheduleStep::StageBarrier(stage) => println!("publication stage: {stage:?}"),
        }
    }
    for system in plan.systems() {
        println!("registered system: {}", system.definition.id);
    }
    Ok(())
}
