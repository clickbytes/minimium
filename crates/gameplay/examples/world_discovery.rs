use minimium_gameplay::{
    CommandViewError, CommandViewRequest, EngineModule, GameplaySystem, GameplaySystemContext,
    ModuleCommandHandler, ModuleDescriptor, ModuleError, ModuleRegistrar, ModuleRegistry, Stage,
    SystemDefinition,
};
use minimium_world::entity::{
    ComponentDescriptor, ComponentError, ComponentMutationPolicy, ComponentTypeId,
    ReplicationPolicy, SchemaVersion,
};

const DISCOVERY_PAGE_ROWS: usize = 8;
const DISCOVERY_PAGE_BYTES: usize = 64 * 1024;
const DISCOVERY_PAGE_BUDGET: usize = 4;
const SAMPLE_COMPONENT: ComponentTypeId = match ComponentTypeId::new(0xE050_05) {
    Some(id) => id,
    None => unreachable!(),
};
const SAMPLE_SCHEMA: SchemaVersion = match SchemaVersion::new(1) {
    Some(schema) => schema,
    None => unreachable!(),
};

fn validate_sample_component(_bytes: &[u8]) -> Result<(), ComponentError> {
    Ok(())
}

/// A host-neutral example module with no knowledge of any configured world ID.
pub struct WorldDiscoveryModule;

impl EngineModule<ModuleCommandHandler, GameplaySystem> for WorldDiscoveryModule {
    fn descriptor(&self) -> ModuleDescriptor {
        ModuleDescriptor {
            id: "world-discovery-example",
            version: 1,
            requires: &[],
        }
    }

    fn register(
        &self,
        registrar: &mut ModuleRegistrar<'_, ModuleCommandHandler, GameplaySystem>,
    ) -> Result<(), ModuleError> {
        registrar.component(ComponentDescriptor {
            id: SAMPLE_COMPONENT,
            name: "example.discovery_marker",
            schema: SAMPLE_SCHEMA,
            validate_payload: validate_sample_component,
            replication: ReplicationPolicy::ServerOnly,
            visibility: minimium_world::entity::ReplicationVisibility::Global,
            mutation_policy: ComponentMutationPolicy::General,
        })?;
        registrar.system(SystemDefinition {
            id: "example-world-discovery",
            stage: Stage::Environment,
            before: &[],
            after: &[],
            handler: discover_worlds,
        })
    }
}

fn discover_worlds(context: &mut dyn GameplaySystemContext) {
    let mut cursor = None;

    for _ in 0..DISCOVERY_PAGE_BUDGET {
        let request = match CommandViewRequest::new(DISCOVERY_PAGE_ROWS, DISCOVERY_PAGE_BYTES) {
            Ok(request) => match cursor.take() {
                Some(next) => request.after(next),
                None => request,
            },
            Err(_) => return,
        };
        let page = match context.worlds(&request) {
            Ok(page) => page,
            Err(CommandViewError::StaleCursor) => return,
            Err(_) => return,
        };
        for world in page.rows {
            let _terrain = context.terrain_read(world.id);
        }
        let Some(next) = page.next else {
            break;
        };
        cursor = Some(next);
    }
}

fn main() -> Result<(), ModuleError> {
    let mut registry = ModuleRegistry::<ModuleCommandHandler, GameplaySystem>::default();
    registry.install(&WorldDiscoveryModule)
}
