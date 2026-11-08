use crate::{
    ComponentView, EffectBufferError, GameplayEvent, InitialEntityView, Stage, WorldEffect,
    WorldLifecycle,
};
use minimium_world::entity::{ComponentTypeId, EntityId};
use minimium_world::world::WorldInstanceId;

/// Narrow, read-only view and bounded effect port exposed to game systems.
pub trait GameplaySystemContext {
    fn stage(&self) -> Stage;
    fn simulation_tick(&self) -> u64;
    fn world_instance(&self) -> Option<WorldInstanceId>;
    fn world_lifecycle(&self) -> Option<WorldLifecycle>;
    fn world_count(&self) -> usize;
    fn world_initialized(&self, instance: WorldInstanceId) -> Option<bool>;
    fn components(&self, instance: WorldInstanceId) -> Vec<ComponentView<'_>>;
    fn component(
        &self,
        instance: WorldInstanceId,
        entity: EntityId,
        kind: ComponentTypeId,
    ) -> Option<ComponentView<'_>>;
    fn initial_entities(&self, instance: WorldInstanceId) -> Vec<InitialEntityView<'_>>;
    fn events(&self) -> &[GameplayEvent];
    fn record_startup_run(&mut self, world_count: usize);
    fn emit(&mut self, effect: WorldEffect) -> Result<(), EffectBufferError>;
}

pub type GameplaySystem = for<'a> fn(&mut dyn GameplaySystemContext);
