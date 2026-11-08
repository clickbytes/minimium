use minimium_gameplay::{
    ComponentValue, EngineModule, GameplayEvent, GameplaySystem, GameplaySystemContext,
    ModuleDescriptor, ModuleError, ModuleRegistrar, Stage, SystemDefinition, WorldEffect,
    WorldLifecycle,
};
use minimium_world::entity::{
    ComponentDescriptor, ComponentTypeId, ReplicationPolicy, SchemaVersion,
};
use minimium_world::world::WorldInstanceId;

pub const PORTAL_COMPONENT: ComponentTypeId = match ComponentTypeId::new(1) {
    Some(id) => id,
    None => unreachable!(),
};
pub const PORTAL_SCHEMA: SchemaVersion = match SchemaVersion::new(1) {
    Some(version) => version,
    None => unreachable!(),
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PortalComponent {
    pub position: [f32; 3],
    pub target_world: WorldInstanceId,
    pub destination: [f32; 3],
}

impl PortalComponent {
    pub fn encode(self) -> Option<Vec<u8>> {
        if self
            .position
            .iter()
            .chain(self.destination.iter())
            .any(|value| !value.is_finite())
        {
            return None;
        }
        let mut bytes = Vec::with_capacity(40);
        for value in self.position {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&self.target_world.get().to_le_bytes());
        for value in self.destination {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        Some(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != 40 {
            return None;
        }
        let position = [
            read_f32(bytes, 0)?,
            read_f32(bytes, 4)?,
            read_f32(bytes, 8)?,
        ];
        let target_world =
            WorldInstanceId::new(u128::from_le_bytes(bytes[12..28].try_into().ok()?))?;
        let destination = [
            read_f32(bytes, 28)?,
            read_f32(bytes, 32)?,
            read_f32(bytes, 36)?,
        ];
        Some(Self {
            position,
            target_world,
            destination,
        })
    }
}

fn read_f32(bytes: &[u8], offset: usize) -> Option<f32> {
    let value = f32::from_le_bytes(bytes.get(offset..offset + 4)?.try_into().ok()?);
    value.is_finite().then_some(value)
}

/// Registers sample-game component schemas with the host runtime.
pub struct SampleGameplayModule;

impl<H> EngineModule<H, GameplaySystem> for SampleGameplayModule {
    fn descriptor(&self) -> ModuleDescriptor {
        ModuleDescriptor {
            id: "sample",
            version: 1,
            requires: &[],
        }
    }

    fn register(
        &self,
        registrar: &mut ModuleRegistrar<'_, H, GameplaySystem>,
    ) -> Result<(), ModuleError> {
        registrar.component(ComponentDescriptor {
            id: PORTAL_COMPONENT,
            name: "sample.portal",
            schema: PORTAL_SCHEMA,
            replication: ReplicationPolicy::Replicated,
            visibility: minimium_world::entity::ReplicationVisibility::Spatial {
                position: portal_position,
            },
        })?;
        registrar.system(SystemDefinition {
            id: "sample-world-startup",
            stage: Stage::Startup,
            before: &[],
            after: &[],
            handler: initialize_sample_entities,
        })?;
        registrar.system(SystemDefinition {
            id: "portal-interaction",
            stage: Stage::PostSimulation,
            before: &[],
            after: &[],
            handler: portal_interaction_system,
        })
    }
}

fn initialize_sample_entities(context: &mut dyn GameplaySystemContext) {
    if context.stage() != Stage::Startup || context.world_lifecycle() != Some(WorldLifecycle::New) {
        return;
    }
    let Some(world) = context.world_instance() else {
        return;
    };
    let entities = context
        .initial_entities(world)
        .into_iter()
        .map(|entity| {
            let components = entity
                .components
                .into_iter()
                .map(|component| ComponentValue {
                    kind: component.kind,
                    schema: component.schema,
                    bytes: component.bytes.to_vec(),
                })
                .collect();
            (entity.sector, components)
        })
        .collect::<Vec<_>>();
    for (sector, components) in entities {
        let _ = context.emit(WorldEffect::SpawnEntity {
            world,
            sector,
            components,
        });
    }
}

fn portal_position(bytes: &[u8]) -> Option<[f32; 3]> {
    PortalComponent::decode(bytes).map(|portal| portal.position)
}

fn portal_interaction_system(context: &mut dyn GameplaySystemContext) {
    if context.stage() != Stage::PostSimulation {
        return;
    }
    let events = context.events().to_vec();
    for event in events {
        let GameplayEvent::PortalUseRequested {
            player,
            world,
            entity,
            position,
        } = event;
        let Some(component) = context.component(world, entity, PORTAL_COMPONENT) else {
            continue;
        };
        if component.schema != PORTAL_SCHEMA {
            continue;
        }
        let Some(portal) = PortalComponent::decode(component.bytes) else {
            continue;
        };
        let distance_squared = position
            .iter()
            .zip(portal.position)
            .map(|(player, portal)| (player - portal).powi(2))
            .sum::<f32>();
        if distance_squared > 9.0 {
            continue;
        }
        let _ = context.emit(WorldEffect::RequestTransfer {
            player,
            source_world: world,
            target_world: portal.target_world,
            destination: portal.destination,
        });
    }
}
