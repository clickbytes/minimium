use crate::{ComponentView, EffectBufferError, WorldEffect, WorldEffectBuffer};
use minimium_core::ids::PlayerId;
use minimium_world::{
    entity::EntityId,
    world::{GenerationRevision, WorldInstanceId, WorldManifest, WorldTopology},
};

const MAX_MODULE_COMMAND_VIEW_ROWS: usize = 256;
const MAX_MODULE_COMMAND_VIEW_BYTES: usize = 1024 * 1024;

/// Bounded UTF-8 command text shared by neutral modules and the host formatter.
#[derive(Clone, Debug, Default)]
pub struct ModuleCommandText(String, bool);

impl ModuleCommandText {
    pub const LIMIT: usize = 8 * 1024;
    const MARKER: &'static str = " [truncated]";

    pub fn new(text: &str) -> Self {
        let mut value = Self::default();
        value.push_str(text);
        value
    }

    pub fn push_str(&mut self, text: &str) {
        if self.1 {
            return;
        }
        let remaining = Self::LIMIT.saturating_sub(self.0.len());
        let mut end = text.len().min(remaining);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        self.0.push_str(&text[..end]);
        if end < text.len() {
            let mut boundary = Self::LIMIT
                .saturating_sub(Self::MARKER.len())
                .min(self.0.len());
            while !self.0.is_char_boundary(boundary) {
                boundary -= 1;
            }
            self.0.truncate(boundary);
            self.0.push_str(Self::MARKER);
            self.1 = true;
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn is_truncated(&self) -> bool {
        self.1
    }
}

impl std::fmt::Write for ModuleCommandText {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        self.push_str(text);
        Ok(())
    }
}

/// A neutral command result; hosts adapt it to their existing reply format.
#[derive(Clone, Debug)]
pub enum ModuleCommandResult {
    Success(ModuleCommandText),
    Error {
        code: ModuleCommandErrorCode,
        detail: ModuleCommandText,
    },
}

/// Validated stable identifier for a module command failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModuleCommandErrorCode(&'static str);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModuleCommandErrorCodeError;

impl ModuleCommandErrorCode {
    pub fn new(code: &'static str) -> Result<Self, ModuleCommandErrorCodeError> {
        if code.is_empty()
            || code.len() > 128
            || !code
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        {
            return Err(ModuleCommandErrorCodeError);
        }
        Ok(Self(code))
    }

    pub fn as_str(self) -> &'static str {
        self.0
    }
}

impl ModuleCommandResult {
    pub fn success(text: &str) -> Self {
        Self::Success(ModuleCommandText::new(text))
    }

    pub fn error(code: ModuleCommandErrorCode, detail: &str) -> Self {
        Self::Error {
            code,
            detail: ModuleCommandText::new(detail),
        }
    }
}

pub type ModuleCommandHandler = fn(&mut ModuleCommandContext<'_>, &[&str]) -> ModuleCommandResult;

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
