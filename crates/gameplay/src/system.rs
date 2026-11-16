use crate::{
    CommandViewError, CommandViewPage, CommandViewRequest, CommandWorldView, ComponentView,
    EffectBufferError, GameplayEvent, InitialEntityView, Stage, WorldEffect, WorldLifecycle,
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
    /// Lists worlds through the host's bounded, revision-aware read model.
    fn worlds(
        &self,
        request: &CommandViewRequest,
    ) -> Result<CommandViewPage<CommandWorldView>, CommandViewError>;
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
    /// Immutable committed surface changes; modules cannot mutate generation or storage.
    fn terrain(&self, _instance: WorldInstanceId) -> Option<&minimium_terrain::TerrainSnapshot> {
        None
    }
    /// Current generated terrain and committed volume edits; unavailable worlds stay unavailable.
    fn terrain_read(
        &self,
        _instance: WorldInstanceId,
    ) -> Option<&dyn minimium_world::terrain::TerrainRead> {
        None
    }
    fn record_startup_run(&mut self, world_count: usize);
    fn emit(&mut self, effect: WorldEffect) -> Result<(), EffectBufferError>;
}

pub type GameplaySystem = for<'a> fn(&mut dyn GameplaySystemContext);
