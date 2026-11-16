use super::gameplay_terrain::{self, CurrentTerrainRead};
use crate::commands::RuntimeCommandReadView;
use crate::modules::{
    BuiltinOperation, GameplaySystem, GameplaySystemContext, SchedulePlan, Stage, WorldLifecycle,
};
use crate::modules::{
    EffectBufferError, EffectProducerId, EffectReceipt, EffectToken, GameplayEvent, WorldEffect,
    WorldEffectBatch, WorldEffectBuffer, MAX_EFFECT_RECEIPTS_PER_PRODUCER,
};
use crate::world_registry::WorldRegistry;
use minimium_gameplay::{
    CommandViewError, CommandViewPage, CommandViewRequest, CommandWorldView, ModuleCommandReadView,
};
use minimium_procgen::PreparedGeography;
use minimium_world::world::WorldInstanceId;
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

const MAX_STARTUP_EFFECT_RECEIPTS: usize = 4096;
const MAX_SYSTEM_EFFECT_RECEIPTS: usize = 4096;

pub type ModuleSystem = GameplaySystem;
pub(crate) type SystemFn = ModuleSystem;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ScheduleDiagnostics {
    pub(crate) world_instance_count: usize,
    pub(crate) startup_system_runs: usize,
    pub(crate) expired_effect_receipts: u64,
    pub(crate) effect_backpressure_rejections: u64,
}

/// Systems receive a read-only world/component view, current gameplay events,
/// a bounded effect buffer and a schedule-owned diagnostic sink. No process,
/// filesystem, network or GPU handle is exposed.
pub struct ModuleSystemContext<'a> {
    stage: Stage,
    worlds: &'a WorldRegistry,
    world_reader: &'a dyn ModuleCommandReadView,
    terrain_reads: &'a std::collections::HashMap<WorldInstanceId, CurrentTerrainRead>,
    diagnostics: &'a mut ScheduleDiagnostics,
    effects: &'a mut WorldEffectBuffer,
    producer: EffectProducerId,
    next_sequence: &'a mut Option<u64>,
    reservations: &'a mut usize,
    startup_pending: &'a mut usize,
    startup_reservations: &'a mut usize,
    system_pending: &'a mut usize,
    system_reservations: &'a mut usize,
    receipts: Vec<EffectReceipt>,
    events: &'a [GameplayEvent],
    simulation_tick: u64,
    world_instance: Option<minimium_world::world::WorldInstanceId>,
    world_lifecycle: Option<WorldLifecycle>,
}

impl ModuleSystemContext<'_> {
    pub fn stage(&self) -> Stage {
        self.stage
    }
    pub fn simulation_tick(&self) -> u64 {
        self.simulation_tick
    }
    pub fn world_instance(&self) -> Option<minimium_world::world::WorldInstanceId> {
        self.world_instance
    }
    pub fn world_lifecycle(&self) -> Option<WorldLifecycle> {
        self.world_lifecycle
    }
    pub fn world_count(&self) -> usize {
        self.worlds.iter().count()
    }
    pub fn worlds(
        &self,
        request: &CommandViewRequest,
    ) -> Result<CommandViewPage<CommandWorldView>, CommandViewError> {
        self.world_reader.worlds(request)
    }
    pub fn world_initialized(
        &self,
        instance: minimium_world::world::WorldInstanceId,
    ) -> Option<bool> {
        self.worlds.content_initialized(instance)
    }
    pub fn components(
        &self,
        instance: minimium_world::world::WorldInstanceId,
    ) -> impl Iterator<Item = crate::modules::ComponentView<'_>> {
        self.worlds
            .component_views(instance)
            .into_iter()
            .flatten()
            .map(
                |(entity, kind, schema, bytes)| crate::modules::ComponentView {
                    entity,
                    kind,
                    schema,
                    bytes,
                },
            )
    }
    pub fn initial_entities(
        &self,
        instance: minimium_world::world::WorldInstanceId,
    ) -> Vec<crate::modules::InitialEntityView<'_>> {
        self.worlds.initial_entities(instance)
    }
    pub fn component(
        &self,
        instance: minimium_world::world::WorldInstanceId,
        entity: minimium_world::entity::EntityId,
        kind: minimium_world::entity::ComponentTypeId,
    ) -> Option<crate::modules::ComponentView<'_>> {
        let blob = self.worlds.component(instance, entity, kind)?;
        Some(crate::modules::ComponentView {
            entity,
            kind,
            schema: blob.schema,
            bytes: &blob.bytes,
        })
    }
    pub fn events(&self) -> &[GameplayEvent] {
        self.events
    }
    pub fn record_startup_run(&mut self, world_count: usize) {
        self.diagnostics.world_instance_count = world_count;
        self.diagnostics.startup_system_runs =
            self.diagnostics.startup_system_runs.saturating_add(1);
    }
    pub fn emit(&mut self, effect: WorldEffect) -> Result<EffectToken, EffectBufferError> {
        let full = if matches!(self.producer, EffectProducerId::Startup { .. }) {
            self.startup_pending
                .saturating_add(*self.startup_reservations)
                >= MAX_STARTUP_EFFECT_RECEIPTS
        } else {
            self.receipts.len().saturating_add(*self.reservations)
                >= MAX_EFFECT_RECEIPTS_PER_PRODUCER
                || self
                    .system_pending
                    .saturating_add(*self.system_reservations)
                    >= MAX_SYSTEM_EFFECT_RECEIPTS
        };
        if full {
            self.diagnostics.effect_backpressure_rejections = self
                .diagnostics
                .effect_backpressure_rejections
                .saturating_add(1);
            return Err(EffectBufferError::ReceiptBackpressure);
        }
        let sequence = (*self.next_sequence).ok_or(EffectBufferError::IdentifierSpaceExhausted)?;
        let token = EffectToken::new(self.producer, sequence);
        self.effects.push_tagged(effect, token)?;
        *self.next_sequence = sequence.checked_add(1);
        if matches!(self.producer, EffectProducerId::Startup { .. }) {
            *self.startup_reservations = self.startup_reservations.saturating_add(1);
        } else {
            *self.reservations = self.reservations.saturating_add(1);
            *self.system_reservations = self.system_reservations.saturating_add(1);
        }
        Ok(token)
    }
}

impl GameplaySystemContext for ModuleSystemContext<'_> {
    fn terrain(
        &self,
        instance: minimium_world::world::WorldInstanceId,
    ) -> Option<&minimium_terrain::TerrainSnapshot> {
        Some(&self.worlds.get(instance)?.terrain)
    }
    fn terrain_read(
        &self,
        instance: WorldInstanceId,
    ) -> Option<&dyn minimium_world::terrain::TerrainRead> {
        self.terrain_reads.get(&instance).map(|read| read as _)
    }
    fn stage(&self) -> Stage {
        self.stage()
    }
    fn simulation_tick(&self) -> u64 {
        self.simulation_tick()
    }
    fn world_instance(&self) -> Option<minimium_world::world::WorldInstanceId> {
        self.world_instance()
    }
    fn world_lifecycle(&self) -> Option<WorldLifecycle> {
        self.world_lifecycle()
    }
    fn world_count(&self) -> usize {
        self.world_count()
    }
    fn worlds(
        &self,
        request: &CommandViewRequest,
    ) -> Result<CommandViewPage<CommandWorldView>, CommandViewError> {
        ModuleSystemContext::worlds(self, request)
    }
    fn world_initialized(&self, instance: minimium_world::world::WorldInstanceId) -> Option<bool> {
        self.world_initialized(instance)
    }
    fn components(
        &self,
        instance: minimium_world::world::WorldInstanceId,
    ) -> Vec<crate::modules::ComponentView<'_>> {
        ModuleSystemContext::components(self, instance).collect()
    }
    fn component(
        &self,
        instance: minimium_world::world::WorldInstanceId,
        entity: minimium_world::entity::EntityId,
        kind: minimium_world::entity::ComponentTypeId,
    ) -> Option<crate::modules::ComponentView<'_>> {
        ModuleSystemContext::component(self, instance, entity, kind)
    }
    fn initial_entities(
        &self,
        instance: minimium_world::world::WorldInstanceId,
    ) -> Vec<crate::modules::InitialEntityView<'_>> {
        ModuleSystemContext::initial_entities(self, instance)
    }
    fn events(&self) -> &[GameplayEvent] {
        self.events()
    }
    fn take_effect_receipts(&mut self) -> Vec<EffectReceipt> {
        let count = self.receipts.len();
        if matches!(self.producer, EffectProducerId::System { .. }) {
            *self.system_pending = self.system_pending.saturating_sub(count);
        }
        std::mem::take(&mut self.receipts)
    }
    fn record_startup_run(&mut self, world_count: usize) {
        self.record_startup_run(world_count);
    }
    fn emit(&mut self, effect: WorldEffect) -> Result<EffectToken, EffectBufferError> {
        self.emit(effect)
    }
}

struct ScheduledSystem {
    module: &'static str,
    id: &'static str,
    stage: Stage,
    handler: SystemFn,
}

struct ProducerFeedback {
    next_sequence: Option<u64>,
    reservations: usize,
    pending: VecDeque<EffectReceipt>,
    expired: u64,
}

impl Default for ProducerFeedback {
    fn default() -> Self {
        Self {
            next_sequence: Some(0),
            reservations: 0,
            pending: VecDeque::new(),
            expired: 0,
        }
    }
}

pub(crate) struct ServerSchedule {
    operations: Vec<BuiltinOperation>,
    systems: Vec<ScheduledSystem>,
    startup_worlds: std::collections::BTreeSet<minimium_world::world::WorldInstanceId>,
    diagnostics: ScheduleDiagnostics,
    feedback: BTreeMap<EffectProducerId, ProducerFeedback>,
    command_sequences: BTreeMap<EffectProducerId, Option<u64>>,
    startup_receipts: BTreeMap<EffectProducerId, VecDeque<EffectReceipt>>,
    startup_pending_receipts: usize,
    startup_reserved_receipts: usize,
    system_pending_receipts: usize,
    system_reserved_receipts: usize,
}

impl ServerSchedule {
    pub(crate) fn from_plan(plan: &SchedulePlan<'_, SystemFn>) -> Self {
        Self {
            operations: plan.operations().to_vec(),
            systems: plan
                .systems()
                .iter()
                .map(|system| ScheduledSystem {
                    stage: system.definition.stage,
                    module: system.module,
                    id: system.definition.id,
                    handler: system.definition.handler,
                })
                .collect(),
            startup_worlds: std::collections::BTreeSet::new(),
            diagnostics: ScheduleDiagnostics::default(),
            feedback: BTreeMap::new(),
            command_sequences: BTreeMap::new(),
            startup_receipts: BTreeMap::new(),
            startup_pending_receipts: 0,
            startup_reserved_receipts: 0,
            system_pending_receipts: 0,
            system_reserved_receipts: 0,
        }
    }

    pub(crate) fn tick_operation(&self, index: usize) -> Option<BuiltinOperation> {
        self.operations.get(index).copied()
    }

    pub(crate) fn run_stage(
        &mut self,
        stage: Stage,
        simulation_tick: u64,
        worlds: &WorldRegistry,
        geography: Option<&Arc<dyn PreparedGeography>>,
        events: &[GameplayEvent],
        world_context: Option<(minimium_world::world::WorldInstanceId, WorldLifecycle)>,
    ) -> WorldEffectBatch {
        if stage == Stage::Startup {
            let Some((instance, _)) = world_context else {
                return WorldEffectBuffer::default().into_batch();
            };
            if self.startup_worlds.contains(&instance) {
                return WorldEffectBuffer::default().into_batch();
            }
        }
        let mut effects = WorldEffectBuffer::default();
        let terrain_reads = gameplay_terrain::build(worlds, geography);
        let world_reader = RuntimeCommandReadView::new(worlds);
        for system in self.systems.iter().filter(|system| system.stage == stage) {
            if let Some((world, _)) = world_context {
                let producer = EffectProducerId::Startup {
                    module: system.module,
                    system: system.id,
                    world,
                };
                let mut next_sequence = Some(0);
                let mut reservations = 0;
                let mut context = ModuleSystemContext {
                    stage,
                    worlds,
                    world_reader: &world_reader,
                    terrain_reads: &terrain_reads,
                    diagnostics: &mut self.diagnostics,
                    effects: &mut effects,
                    producer,
                    next_sequence: &mut next_sequence,
                    reservations: &mut reservations,
                    startup_pending: &mut self.startup_pending_receipts,
                    startup_reservations: &mut self.startup_reserved_receipts,
                    system_pending: &mut self.system_pending_receipts,
                    system_reservations: &mut self.system_reserved_receipts,
                    receipts: Vec::new(),
                    events,
                    simulation_tick,
                    world_instance: Some(world),
                    world_lifecycle: world_context.map(|(_, life)| life),
                };
                (system.handler)(&mut context);
            } else {
                let producer = EffectProducerId::System {
                    module: system.module,
                    system: system.id,
                };
                let expired = {
                    let state = self.feedback.entry(producer).or_default();
                    let receipts: Vec<EffectReceipt> = state.pending.drain(..).collect();
                    let mut context = ModuleSystemContext {
                        stage,
                        worlds,
                        world_reader: &world_reader,
                        terrain_reads: &terrain_reads,
                        diagnostics: &mut self.diagnostics,
                        effects: &mut effects,
                        producer,
                        next_sequence: &mut state.next_sequence,
                        reservations: &mut state.reservations,
                        startup_pending: &mut self.startup_pending_receipts,
                        startup_reservations: &mut self.startup_reserved_receipts,
                        system_pending: &mut self.system_pending_receipts,
                        system_reservations: &mut self.system_reserved_receipts,
                        receipts,
                        events,
                        simulation_tick,
                        world_instance: None,
                        world_lifecycle: None,
                    };
                    (system.handler)(&mut context);
                    let expired = context.receipts.len() as u64;
                    *context.system_pending =
                        context.system_pending.saturating_sub(expired as usize);
                    expired
                };
                if let Some(state) = self.feedback.get_mut(&producer) {
                    state.expired = state.expired.saturating_add(expired);
                }
                self.diagnostics.expired_effect_receipts = self
                    .diagnostics
                    .expired_effect_receipts
                    .saturating_add(expired);
            }
        }
        let batch = effects.into_batch();
        if stage == Stage::Startup && batch.rejected_pushes() == 0 {
            if let Some((instance, _)) = world_context {
                self.startup_worlds.insert(instance);
            }
        }
        batch
    }

    pub(crate) const fn diagnostics(&self) -> ScheduleDiagnostics {
        self.diagnostics
    }

    pub(crate) fn publish_effect_receipts(
        &mut self,
        outcome: &crate::modules::EffectBatchOutcome,
    ) -> Result<(), std::io::Error> {
        for receipt in outcome.receipts() {
            match receipt.token.producer() {
                producer @ EffectProducerId::System { .. } => {
                    let state = self.feedback.get_mut(&producer).ok_or_else(|| {
                        std::io::Error::other(
                            "committed system receipt has no producer reservation",
                        )
                    })?;
                    if state.reservations == 0 {
                        return Err(std::io::Error::other(
                            "committed system receipt reservation underflow",
                        ));
                    }
                    if self.system_reserved_receipts == 0 {
                        return Err(std::io::Error::other(
                            "committed system receipt has no global reservation",
                        ));
                    }
                    state.reservations -= 1;
                    self.system_reserved_receipts -= 1;
                    self.system_pending_receipts += 1;
                    state.pending.push_back(receipt.clone());
                }
                producer @ EffectProducerId::Startup { .. } => {
                    if self.startup_reserved_receipts == 0 {
                        return Err(std::io::Error::other(
                            "committed startup receipt has no global reservation",
                        ));
                    }
                    self.startup_reserved_receipts -= 1;
                    self.startup_pending_receipts += 1;
                    self.startup_receipts
                        .entry(producer)
                        .or_default()
                        .push_back(receipt.clone());
                }
                EffectProducerId::Command { .. } => {}
            }
        }
        Ok(())
    }

    pub(crate) fn take_startup_effect_receipts(
        &mut self,
        module: &'static str,
        system: &'static str,
        world: WorldInstanceId,
    ) -> Vec<EffectReceipt> {
        let producer = EffectProducerId::Startup {
            module,
            system,
            world,
        };
        let receipts: Vec<_> = self
            .startup_receipts
            .remove(&producer)
            .unwrap_or_default()
            .into_iter()
            .collect();
        self.startup_pending_receipts =
            self.startup_pending_receipts.saturating_sub(receipts.len());
        receipts
    }

    pub(crate) fn command_sequences_mut(&mut self) -> &mut BTreeMap<EffectProducerId, Option<u64>> {
        &mut self.command_sequences
    }

    pub(crate) fn drain_pending_effect_receipts(&mut self) -> Vec<EffectReceipt> {
        let mut receipts = Vec::new();
        for state in self.feedback.values_mut() {
            receipts.extend(state.pending.drain(..));
        }
        for queue in self.startup_receipts.values_mut() {
            receipts.extend(queue.drain(..));
        }
        self.startup_pending_receipts = 0;
        self.system_pending_receipts = 0;
        self.startup_receipts.clear();
        receipts
    }
}
