use minimium_core::ids::PlayerId;
use minimium_world::{
    entity::{ComponentTypeId, EntityId, SchemaVersion},
    world::{SectorKey, WorldInstanceId},
};

pub const MAX_WORLD_EFFECTS_PER_STAGE: usize = 256;
pub const MAX_WORLD_EFFECT_BYTES_PER_STAGE: usize = 1024 * 1024;
pub const MAX_EFFECT_RECEIPTS_PER_PRODUCER: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum EffectProducerId {
    System {
        module: &'static str,
        system: &'static str,
    },
    Command {
        module: &'static str,
        command: &'static str,
    },
    Startup {
        module: &'static str,
        system: &'static str,
        world: WorldInstanceId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct EffectToken {
    producer: EffectProducerId,
    sequence: u64,
}

impl EffectToken {
    /// Constructs the token selected by the host-side registered producer adapter.
    pub const fn new(producer: EffectProducerId, sequence: u64) -> Self {
        Self { producer, sequence }
    }
    pub const fn producer(self) -> EffectProducerId {
        self.producer
    }
    pub const fn sequence(self) -> u64 {
        self.sequence
    }
}

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
    ReceiptBackpressure,
    IdentifierSpaceExhausted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntityEffectRejection {
    Validation,
    StorageBudget,
    UnknownInstance,
    UnknownEntity,
    ProtectedComponent,
    ReservationConflict,
    IdentifierSpaceExhausted,
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
    EntitySpawned {
        world: WorldInstanceId,
        entity: EntityId,
    },
    EntityRejected(EntityEffectRejection),
    Transfer(TransferOutcome),
    StageRejected,
}

#[derive(Default)]
pub struct WorldEffectBuffer {
    effects: Vec<WorldEffect>,
    tokens: Vec<EffectToken>,
    payload_bytes: usize,
    rejected: usize,
}

impl WorldEffectBuffer {
    pub fn push_tagged(
        &mut self,
        effect: WorldEffect,
        token: EffectToken,
    ) -> Result<(), EffectBufferError> {
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
        self.tokens.push(token);
        Ok(())
    }

    pub fn into_batch(self) -> WorldEffectBatch {
        WorldEffectBatch {
            effects: self.effects,
            tokens: self.tokens,
            rejected: self.rejected,
        }
    }
}

pub struct WorldEffectBatch {
    effects: Vec<WorldEffect>,
    tokens: Vec<EffectToken>,
    rejected: usize,
}

impl WorldEffectBatch {
    pub fn effects(&self) -> &[WorldEffect] {
        &self.effects
    }

    pub const fn rejected_pushes(&self) -> usize {
        self.rejected
    }

    pub fn token_at(&self, index: usize) -> Option<EffectToken> {
        self.tokens.get(index).copied()
    }

    pub fn into_outcome(
        self,
        results: Vec<WorldEffectOutcome>,
    ) -> Result<EffectBatchOutcome, EffectOutcomeError> {
        if self.effects.len() > MAX_WORLD_EFFECTS_PER_STAGE || results.len() != self.effects.len() {
            return Err(EffectOutcomeError::ResultCountMismatch);
        }
        let receipts = self
            .tokens
            .iter()
            .zip(results.iter())
            .map(|(token, outcome)| EffectReceipt {
                token: *token,
                outcome: *outcome,
            })
            .collect();
        Ok(EffectBatchOutcome {
            results,
            rejected_pushes: self.rejected,
            receipts,
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
    receipts: Vec<EffectReceipt>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectReceipt {
    pub token: EffectToken,
    pub outcome: WorldEffectOutcome,
}

impl EffectBatchOutcome {
    pub fn results(&self) -> &[WorldEffectOutcome] {
        &self.results
    }

    pub const fn rejected_pushes(&self) -> usize {
        self.rejected_pushes
    }

    pub fn receipts(&self) -> &[EffectReceipt] {
        &self.receipts
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

    pub fn has_successes(&self) -> bool {
        self.results.iter().any(|result| {
            matches!(
                result,
                WorldEffectOutcome::EntityApplied
                    | WorldEffectOutcome::EntitySpawned { .. }
                    | WorldEffectOutcome::Transfer(TransferOutcome::Started)
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
