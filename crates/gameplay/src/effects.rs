use minimium_core::ids::PlayerId;
use minimium_world::{
    entity::{ComponentTypeId, EntityId, SchemaVersion},
    world::{SectorKey, WorldInstanceId},
};

pub const MAX_WORLD_EFFECTS_PER_STAGE: usize = 256;
pub const MAX_WORLD_EFFECT_BYTES_PER_STAGE: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentValue {
    pub kind: ComponentTypeId,
    pub schema: SchemaVersion,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Copy)]
pub struct ComponentView<'a> {
    pub entity: EntityId,
    pub kind: ComponentTypeId,
    pub schema: SchemaVersion,
    pub bytes: &'a [u8],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldLifecycle {
    New,
    Restored,
}

#[derive(Clone, Copy)]
pub struct InitialComponentView<'a> {
    pub kind: ComponentTypeId,
    pub schema: SchemaVersion,
    pub bytes: &'a [u8],
}

pub struct InitialEntityView<'a> {
    pub sector: SectorKey,
    pub components: Vec<InitialComponentView<'a>>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum WorldEffect {
    SpawnEntity {
        world: WorldInstanceId,
        sector: SectorKey,
        components: Vec<ComponentValue>,
    },
    DespawnEntity {
        world: WorldInstanceId,
        entity: EntityId,
    },
    SetComponent {
        world: WorldInstanceId,
        entity: EntityId,
        component: ComponentValue,
    },
    RemoveComponent {
        world: WorldInstanceId,
        entity: EntityId,
        kind: ComponentTypeId,
    },
    RequestTransfer {
        player: PlayerId,
        source_world: WorldInstanceId,
        target_world: WorldInstanceId,
        destination: [f32; 3],
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EffectBufferError {
    OperationLimit,
    PayloadLimit,
}

#[derive(Default)]
pub struct WorldEffectBuffer {
    effects: Vec<WorldEffect>,
    payload_bytes: usize,
    rejected: usize,
}

impl WorldEffectBuffer {
    pub fn push(&mut self, effect: WorldEffect) -> Result<(), EffectBufferError> {
        let payload = match &effect {
            WorldEffect::SpawnEntity { components, .. } => {
                components.iter().fold(0usize, |total, component| {
                    total.saturating_add(component.bytes.len())
                })
            }
            WorldEffect::SetComponent { component, .. } => component.bytes.len(),
            _ => 0,
        };
        if self.effects.len() >= MAX_WORLD_EFFECTS_PER_STAGE {
            self.rejected = self.rejected.saturating_add(1);
            return Err(EffectBufferError::OperationLimit);
        }
        let Some(total) = self.payload_bytes.checked_add(payload) else {
            self.rejected = self.rejected.saturating_add(1);
            return Err(EffectBufferError::PayloadLimit);
        };
        if total > MAX_WORLD_EFFECT_BYTES_PER_STAGE {
            self.rejected = self.rejected.saturating_add(1);
            return Err(EffectBufferError::PayloadLimit);
        }
        self.payload_bytes = total;
        self.effects.push(effect);
        Ok(())
    }

    pub fn into_batch(self) -> WorldEffectBatch {
        WorldEffectBatch {
            effects: self.effects,
            rejected: self.rejected,
        }
    }
}

pub struct WorldEffectBatch {
    pub effects: Vec<WorldEffect>,
    pub rejected: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GameplayEvent {
    PortalUseRequested {
        player: PlayerId,
        world: WorldInstanceId,
        entity: EntityId,
        position: [f32; 3],
    },
}
