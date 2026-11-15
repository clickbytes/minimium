use crate::{ComponentView, EffectBufferError, WorldEffect, WorldEffectBuffer};
use minimium_core::ids::PlayerId;
use minimium_world::{
    entity::EntityId,
    world::{GenerationRevision, WorldInstanceId, WorldManifest, WorldTopology},
};

const MAX_MODULE_COMMAND_VIEW_ROWS: usize = 256;
const MAX_MODULE_COMMAND_VIEW_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug)]
pub struct CommandViewPage<T> {
    pub rows: Vec<T>,
    pub truncated: bool,
}

impl<T> CommandViewPage<T> {
    pub fn new(rows: Vec<T>, truncated: bool) -> Self {
        Self { rows, truncated }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandViewError {
    UnknownWorld,
    UnknownEntity,
}

#[derive(Clone, Copy, Debug)]
pub struct CommandWorldView {
    pub id: WorldInstanceId,
    pub players: usize,
    pub topology: WorldTopology,
    pub generation: GenerationRevision,
    pub manifest: WorldManifest,
}

#[derive(Clone, Debug)]
pub struct CommandPlayerView {
    pub id: PlayerId,
    pub name: String,
    pub world: WorldInstanceId,
    pub position: [f32; 3],
}

pub trait ModuleCommandReadView {
    fn world_count(&self) -> usize;
    fn worlds(&self, max_rows: usize) -> CommandViewPage<CommandWorldView>;
    fn world(&self, id: WorldInstanceId) -> Result<CommandWorldView, CommandViewError>;
    fn entities(
        &self,
        world: WorldInstanceId,
        max_rows: usize,
    ) -> Result<CommandViewPage<EntityId>, CommandViewError>;
    fn players(&self, max_rows: usize, max_bytes: usize) -> CommandViewPage<CommandPlayerView>;
    fn components(
        &self,
        world: WorldInstanceId,
        entity: EntityId,
        max_rows: usize,
        max_bytes: usize,
    ) -> Result<CommandViewPage<ComponentView<'_>>, CommandViewError>;
}

/// A host supplies only read views and a bounded effect sink to a command handler.
pub struct ModuleCommandContext<'a> {
    view: &'a dyn ModuleCommandReadView,
    effects: &'a mut WorldEffectBuffer,
}

impl<'a> ModuleCommandContext<'a> {
    pub fn new(view: &'a dyn ModuleCommandReadView, effects: &'a mut WorldEffectBuffer) -> Self {
        Self { view, effects }
    }

    pub fn world_count(&self) -> usize {
        self.view.world_count()
    }

    pub fn worlds(&self) -> CommandViewPage<CommandWorldView> {
        self.view.worlds(MAX_MODULE_COMMAND_VIEW_ROWS)
    }

    pub fn world(&self, id: WorldInstanceId) -> Result<CommandWorldView, CommandViewError> {
        self.view.world(id)
    }

    pub fn entities(
        &self,
        world: WorldInstanceId,
    ) -> Result<CommandViewPage<EntityId>, CommandViewError> {
        self.view.entities(world, MAX_MODULE_COMMAND_VIEW_ROWS)
    }

    pub fn players(&self) -> CommandViewPage<CommandPlayerView> {
        self.view
            .players(MAX_MODULE_COMMAND_VIEW_ROWS, MAX_MODULE_COMMAND_VIEW_BYTES)
    }

    pub fn components(
        &self,
        world: WorldInstanceId,
        entity: EntityId,
    ) -> Result<CommandViewPage<ComponentView<'_>>, CommandViewError> {
        self.view.components(
            world,
            entity,
            MAX_MODULE_COMMAND_VIEW_ROWS,
            MAX_MODULE_COMMAND_VIEW_BYTES,
        )
    }

    pub fn emit(&mut self, effect: WorldEffect) -> Result<(), EffectBufferError> {
        self.effects.push(effect)
    }
}
