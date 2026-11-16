use crate::{ComponentView, EffectBufferError, WorldEffect, WorldEffectBuffer};
use minimium_world::{entity::EntityId, world::WorldInstanceId};

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

use crate::effects::{EffectProducerId, EffectToken};
use crate::read_views::{
    CommandEntityRow, CommandPlayerView, CommandViewCursor, CommandViewError, CommandViewPage,
    CommandViewRequest, CommandWorldView, ModuleCommandReadView,
};

/// A host supplies only read views and a bounded effect sink to a command handler.
pub struct ModuleCommandContext<'a> {
    view: &'a dyn ModuleCommandReadView,
    effects: &'a mut WorldEffectBuffer,
    producer: EffectProducerId,
    next_sequence: &'a mut Option<u64>,
}

impl<'a> ModuleCommandContext<'a> {
    pub fn new(
        view: &'a dyn ModuleCommandReadView,
        effects: &'a mut WorldEffectBuffer,
        producer: EffectProducerId,
        next_sequence: &'a mut Option<u64>,
    ) -> Self {
        Self {
            view,
            effects,
            producer,
            next_sequence,
        }
    }

    pub fn world_count(&self) -> usize {
        self.view.world_count()
    }

    pub fn cursor(&self, token: &str) -> Result<CommandViewCursor, CommandViewError> {
        self.view.parse_cursor(token)
    }

    pub fn worlds(
        &self,
        request: &CommandViewRequest,
    ) -> Result<CommandViewPage<CommandWorldView>, CommandViewError> {
        self.view.worlds(request)
    }

    pub fn world(&self, id: WorldInstanceId) -> Result<CommandWorldView, CommandViewError> {
        self.view.world(id)
    }

    pub fn entities(
        &self,
        world: WorldInstanceId,
        request: &CommandViewRequest,
    ) -> Result<CommandViewPage<EntityId>, CommandViewError> {
        self.view.entities(world, request)
    }

    pub fn players(
        &self,
        request: &CommandViewRequest,
    ) -> Result<CommandViewPage<CommandPlayerView>, CommandViewError> {
        self.view.players(request)
    }

    pub fn entity_rows(
        &self,
        request: &CommandViewRequest,
    ) -> Result<CommandViewPage<CommandEntityRow>, CommandViewError> {
        self.view.entity_rows(request)
    }

    pub fn components(
        &self,
        world: WorldInstanceId,
        entity: EntityId,
        request: &CommandViewRequest,
    ) -> Result<CommandViewPage<ComponentView<'_>>, CommandViewError> {
        self.view.components(world, entity, request)
    }

    pub fn emit(&mut self, effect: WorldEffect) -> Result<EffectToken, EffectBufferError> {
        let sequence = self
            .next_sequence
            .ok_or(EffectBufferError::IdentifierSpaceExhausted)?;
        let token = EffectToken::new(self.producer, sequence);
        self.effects.push_tagged(effect, token)?;
        *self.next_sequence = sequence.checked_add(1);
        Ok(token)
    }
}
