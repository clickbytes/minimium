use minimium_gameplay::{
    CommandViewRequest, EngineModule, EntityEffectRejection, GameplaySystemContext,
    ModuleDescriptor, ModuleError, ModuleRegistrar, Stage, SystemDefinition, WorldEffect,
    WorldEffectOutcome,
};
use minimium_server_runtime::ModuleSystem;
use minimium_world::entity::{
    ComponentDescriptor, ComponentError, ComponentMutationPolicy, ComponentTypeId, EntityId,
    ReplicationPolicy, ReplicationVisibility, SchemaVersion,
};

pub const FIRST_COMPONENT: ComponentTypeId = match ComponentTypeId::new(0xE050_1601) {
    Some(value) => value,
    None => panic!("component type IDs are nonzero"),
};
pub const SECOND_COMPONENT: ComponentTypeId = match ComponentTypeId::new(0xE050_1602) {
    Some(value) => value,
    None => panic!("component type IDs are nonzero"),
};
pub const GENERAL_COMPONENT: ComponentTypeId = match ComponentTypeId::new(0xE050_1603) {
    Some(value) => value,
    None => panic!("component type IDs are nonzero"),
};
pub const DEMO_SCHEMA: SchemaVersion = match SchemaVersion::new(1) {
    Some(value) => value,
    None => panic!("schema versions are nonzero"),
};

const FIRST_MODULE: &str = "owned-component-alpha";
const SECOND_MODULE: &str = "owned-component-beta";
const STARTUP_SYSTEM: &str = "owned-component-spawn";
const ORDINARY_SYSTEM: &str = "owned-component-attempts";

pub struct FirstOwnerModule;
pub struct SecondOwnerModule;

impl EngineModule<minimium_gameplay::ModuleCommandHandler, ModuleSystem> for FirstOwnerModule {
    fn descriptor(&self) -> ModuleDescriptor {
        ModuleDescriptor {
            id: FIRST_MODULE,
            version: 1,
            requires: &[],
        }
    }

    fn register(
        &self,
        registrar: &mut ModuleRegistrar<'_, minimium_gameplay::ModuleCommandHandler, ModuleSystem>,
    ) -> Result<(), ModuleError> {
        registrar.component(descriptor(
            FIRST_COMPONENT,
            "example.alpha_owned",
            ComponentMutationPolicy::OwnerOnly,
        ))?;
        registrar.component(descriptor(
            GENERAL_COMPONENT,
            "example.general",
            ComponentMutationPolicy::General,
        ))?;
        registrar.system(SystemDefinition {
            id: STARTUP_SYSTEM,
            stage: Stage::Startup,
            before: &[],
            after: &[],
            handler: spawn_empty_entities,
        })?;
        registrar.system(SystemDefinition {
            id: ORDINARY_SYSTEM,
            stage: Stage::Environment,
            before: &[],
            after: &[],
            handler: try_ordinary_mutations,
        })
    }
}

impl EngineModule<minimium_gameplay::ModuleCommandHandler, ModuleSystem> for SecondOwnerModule {
    fn descriptor(&self) -> ModuleDescriptor {
        ModuleDescriptor {
            id: SECOND_MODULE,
            version: 1,
            requires: &[],
        }
    }

    fn register(
        &self,
        registrar: &mut ModuleRegistrar<'_, minimium_gameplay::ModuleCommandHandler, ModuleSystem>,
    ) -> Result<(), ModuleError> {
        registrar.component(descriptor(
            SECOND_COMPONENT,
            "example.beta_owned",
            ComponentMutationPolicy::OwnerOnly,
        ))
    }
}

fn descriptor(
    id: ComponentTypeId,
    name: &'static str,
    mutation_policy: ComponentMutationPolicy,
) -> ComponentDescriptor {
    ComponentDescriptor {
        id,
        name,
        schema: DEMO_SCHEMA,
        validate_payload: validate_demo_payload,
        replication: ReplicationPolicy::ServerOnly,
        visibility: ReplicationVisibility::Global,
        mutation_policy,
    }
}

fn validate_demo_payload(bytes: &[u8]) -> Result<(), ComponentError> {
    (bytes.len() == 1)
        .then_some(())
        .ok_or(ComponentError::InvalidPayload)
}

fn spawn_empty_entities(context: &mut dyn GameplaySystemContext) {
    let Some(world) = context.world_instance() else {
        return;
    };
    for _ in 0..2 {
        let _accepted_token = context.emit(WorldEffect::SpawnEntity {
            world,
            sector: minimium_world::world::SectorKey::dungeon(0, 0, 0),
            components: Vec::new(),
        });
    }
}

fn try_ordinary_mutations(context: &mut dyn GameplaySystemContext) {
    for receipt in context.take_effect_receipts() {
        let expected = match receipt.token.sequence() {
            1 => matches!(
                receipt.outcome,
                WorldEffectOutcome::EntityRejected(EntityEffectRejection::ReservationConflict)
            ),
            2..=3 => matches!(
                receipt.outcome,
                WorldEffectOutcome::EntityRejected(EntityEffectRejection::Validation)
            ),
            4 => matches!(receipt.outcome, WorldEffectOutcome::EntityApplied),
            _ => false,
        };
        assert!(
            expected,
            "ordinary module receipt {:?} had unexpected outcome {:?}",
            receipt.token, receipt.outcome
        );
        println!(
            "ordinary module receipt {:?}: {:?}",
            receipt.token, receipt.outcome
        );
    }

    let Ok(request) = CommandViewRequest::new(16, 16 * 1024) else {
        return;
    };
    let Ok(page) = context.worlds(&request) else {
        return;
    };
    let Some(world) = page.rows.first().map(|row| row.id) else {
        return;
    };
    // The memory backend is empty, so startup's first spawn in each world is entity 1.
    let Some(entity) = EntityId::new(1) else {
        return;
    };
    let effect = match context.simulation_tick() {
        1 => WorldEffect::DespawnEntity { world, entity },
        2 => WorldEffect::SetComponent {
            world,
            entity,
            component: value(FIRST_COMPONENT, 2),
        },
        3 => WorldEffect::RemoveComponent {
            world,
            entity,
            kind: FIRST_COMPONENT,
        },
        4 => WorldEffect::SetComponent {
            world,
            entity,
            component: value(GENERAL_COMPONENT, 4),
        },
        _ => return,
    };
    let _accepted_token_is_not_a_commit_receipt = context.emit(effect);
}

pub fn component_value(kind: ComponentTypeId, byte: u8) -> minimium_gameplay::ComponentValue {
    value(kind, byte)
}

fn value(kind: ComponentTypeId, byte: u8) -> minimium_gameplay::ComponentValue {
    minimium_gameplay::ComponentValue {
        kind,
        schema: DEMO_SCHEMA,
        bytes: vec![byte],
    }
}
