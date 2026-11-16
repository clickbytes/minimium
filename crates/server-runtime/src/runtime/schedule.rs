use super::gameplay_terrain::{self, CurrentTerrainRead};
use crate::commands::RuntimeCommandReadView;
use crate::modules::{
    BuiltinOperation, GameplaySystem, GameplaySystemContext, SchedulePlan, Stage, WorldLifecycle,
};
use crate::modules::{
    EffectBufferError, GameplayEvent, WorldEffect, WorldEffectBatch, WorldEffectBuffer,
};
use crate::world_registry::WorldRegistry;
use minimium_gameplay::{
    CommandViewError, CommandViewPage, CommandViewRequest, CommandWorldView, ModuleCommandReadView,
};
use minimium_procgen::PreparedGeography;
use minimium_world::world::WorldInstanceId;
use std::sync::Arc;

pub type ModuleSystem = GameplaySystem;
pub(crate) type SystemFn = ModuleSystem;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ScheduleDiagnostics {
    pub(crate) world_instance_count: usize,
    pub(crate) startup_system_runs: usize,
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
    pub fn emit(&mut self, effect: WorldEffect) -> Result<(), EffectBufferError> {
        self.effects.push(effect)
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
    fn record_startup_run(&mut self, world_count: usize) {
        self.record_startup_run(world_count);
    }
    fn emit(&mut self, effect: WorldEffect) -> Result<(), EffectBufferError> {
        self.emit(effect)
    }
}

struct ScheduledSystem {
    stage: Stage,
    handler: SystemFn,
}

pub(crate) struct ServerSchedule {
    operations: Vec<BuiltinOperation>,
    systems: Vec<ScheduledSystem>,
    startup_worlds: std::collections::BTreeSet<minimium_world::world::WorldInstanceId>,
    diagnostics: ScheduleDiagnostics,
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
                    handler: system.definition.handler,
                })
                .collect(),
            startup_worlds: std::collections::BTreeSet::new(),
            diagnostics: ScheduleDiagnostics::default(),
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
        let mut context = ModuleSystemContext {
            stage,
            worlds,
            world_reader: &world_reader,
            terrain_reads: &terrain_reads,
            diagnostics: &mut self.diagnostics,
            effects: &mut effects,
            events,
            simulation_tick,
            world_instance: world_context.map(|(instance, _)| instance),
            world_lifecycle: world_context.map(|(_, lifecycle)| lifecycle),
        };
        for system in self.systems.iter().filter(|system| system.stage == stage) {
            (system.handler)(&mut context);
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
}
