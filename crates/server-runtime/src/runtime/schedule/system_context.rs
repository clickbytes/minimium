use super::ModuleSystemContext;
use crate::modules::{
    CommandViewError, CommandViewPage, CommandViewRequest, CommandWorldView, EffectBufferError,
    EffectProducerId, EffectReceipt, EffectToken, GameplayEvent, GameplaySystemContext, Stage,
    WorldEffect, WorldLifecycle,
};
use minimium_world::world::WorldInstanceId;

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
