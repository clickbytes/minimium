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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntityEffectRejection {
    Validation,
    StorageBudget,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferRejection {
    Validation,
    AlreadyPending,
    IdentifierSpaceExhausted,
    ClientUnavailable,
    EnvironmentUnavailable,
    Backpressure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferOutcome {
    Started,
    Rejected(TransferRejection),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorldEffectOutcome {
    EntityApplied,
    EntityRejected(EntityEffectRejection),
    Transfer(TransferOutcome),
    StageRejected,
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
    effects: Vec<WorldEffect>,
    rejected: usize,
}

impl WorldEffectBatch {
    pub fn effects(&self) -> &[WorldEffect] {
        &self.effects
    }

    pub const fn rejected_pushes(&self) -> usize {
        self.rejected
    }

    pub fn into_outcome(
        self,
        results: Vec<WorldEffectOutcome>,
    ) -> Result<EffectBatchOutcome, EffectOutcomeError> {
        if self.effects.len() > MAX_WORLD_EFFECTS_PER_STAGE || results.len() != self.effects.len() {
            return Err(EffectOutcomeError::ResultCountMismatch);
        }
        Ok(EffectBatchOutcome {
            results,
            rejected_pushes: self.rejected,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EffectOutcomeError {
    ResultCountMismatch,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectBatchOutcome {
    results: Vec<WorldEffectOutcome>,
    rejected_pushes: usize,
}

impl EffectBatchOutcome {
    pub fn results(&self) -> &[WorldEffectOutcome] {
        &self.results
    }

    pub const fn rejected_pushes(&self) -> usize {
        self.rejected_pushes
    }

    pub fn has_rejections(&self) -> bool {
        self.rejected_pushes > 0
            || self.results.iter().any(|result| {
                matches!(
                    result,
                    WorldEffectOutcome::EntityRejected(_)
                        | WorldEffectOutcome::Transfer(TransferOutcome::Rejected(_))
                        | WorldEffectOutcome::StageRejected
                )
            })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GameplayEvent {
    EntityInteractionRequested {
        player: PlayerId,
        world: WorldInstanceId,
        entity: EntityId,
        position: [f32; 3],
    },
    /// Compatibility for game modules producing the earlier portal-specific event.
    PortalUseRequested {
        player: PlayerId,
        world: WorldInstanceId,
        entity: EntityId,
        position: [f32; 3],
    },
}
