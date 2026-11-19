use crate::ComponentView;
use minimium_core::ids::PlayerId;
use minimium_world::{
    entity::EntityId,
    world::{GenerationRevision, WorldInstanceId, WorldManifest, WorldTopology},
};
use std::sync::Arc;
mod cursor_codec;

/// Maximum rows and dynamic data accepted by any command read page.
pub const MAX_COMMAND_VIEW_ROWS: usize = 256;
pub const MAX_COMMAND_VIEW_BYTES: usize = 1024 * 1024;

/// A bounded page request. Construct the first request with `new`; reuse the
/// returned cursor for continuation. A zero limit or a limit above the shared
/// caps is rejected before reading.
#[derive(Clone, Debug)]
pub struct CommandViewRequest {
    limit: usize,
    max_bytes: usize,
    cursor: Option<CommandViewCursor>,
}

impl CommandViewRequest {
    pub fn new(limit: usize, max_bytes: usize) -> Result<Self, CommandViewError> {
        if limit == 0
            || limit > MAX_COMMAND_VIEW_ROWS
            || max_bytes == 0
            || max_bytes > MAX_COMMAND_VIEW_BYTES
        {
            return Err(CommandViewError::InvalidLimit);
        }
        Ok(Self {
            limit,
            max_bytes,
            cursor: None,
        })
    }

    pub fn after(mut self, cursor: CommandViewCursor) -> Self {
        self.cursor = Some(cursor);
        self
    }
}

/// Opaque continuation bound to one runtime, query, scope and membership revision.
#[derive(Clone, Debug)]
pub struct CommandViewCursor {
    identity: Arc<()>,
    query: CommandViewKind,
    scope: CommandViewScope,
    revision: u128,
    secondary_revision: u128,
    tertiary_revision: u128,
    after: u128,
}

impl CommandViewCursor {
    pub fn is_same_runtime(&self, identity: &Arc<()>) -> bool {
        Arc::ptr_eq(&self.identity, identity)
    }
    pub fn query(&self) -> CommandViewKind {
        self.query
    }
    pub fn scope(&self) -> CommandViewScope {
        self.scope
    }
    pub fn revision(&self) -> u128 {
        self.revision
    }
    pub fn secondary_revision(&self) -> u128 {
        self.secondary_revision
    }
    pub fn tertiary_revision(&self) -> u128 {
        self.tertiary_revision
    }
    pub fn after_key(&self) -> u128 {
        self.after
    }
    pub fn token(&self) -> String {
        cursor_codec::encode(self)
    }
}

/// Reader-owned capability for issuing cursors tied to that reader's runtime.
#[derive(Clone, Debug)]
pub struct CommandViewCursorIssuer(Arc<()>);

impl CommandViewCursorIssuer {
    pub fn new(identity: Arc<()>) -> Self {
        Self(identity)
    }
    pub fn issue(
        &self,
        query: CommandViewKind,
        scope: CommandViewScope,
        revision: u128,
        after: u128,
    ) -> CommandViewCursor {
        self.issue_versions(query, scope, revision, 0, 0, after)
    }
    pub fn issue_versions(
        &self,
        query: CommandViewKind,
        scope: CommandViewScope,
        revision: u128,
        secondary_revision: u128,
        tertiary_revision: u128,
        after: u128,
    ) -> CommandViewCursor {
        CommandViewCursor {
            identity: self.0.clone(),
            query,
            scope,
            revision,
            secondary_revision,
            tertiary_revision,
            after,
        }
    }
    pub fn parse(&self, token: &str) -> Result<CommandViewCursor, CommandViewError> {
        cursor_codec::decode(self, token)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandViewKind {
    Worlds,
    Entities,
    Players,
    Components,
    EntityRows,
}

impl CommandViewKind {
    fn code(self) -> u8 {
        match self {
            Self::Worlds => 1,
            Self::Entities => 2,
            Self::Players => 3,
            Self::Components => 4,
            Self::EntityRows => 5,
        }
    }
    fn from_code(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Worlds),
            2 => Some(Self::Entities),
            3 => Some(Self::Players),
            4 => Some(Self::Components),
            5 => Some(Self::EntityRows),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandViewScope {
    None,
    World(WorldInstanceId),
    Entity {
        world: WorldInstanceId,
        entity: EntityId,
    },
}

impl CommandViewRequest {
    pub fn limit(&self) -> usize {
        self.limit
    }
    pub fn max_bytes(&self) -> usize {
        self.max_bytes
    }
    pub fn cursor(&self) -> Option<&CommandViewCursor> {
        self.cursor.as_ref()
    }
}

#[derive(Clone, Debug)]
pub struct CommandViewPage<T> {
    pub rows: Vec<T>,
    pub next: Option<CommandViewCursor>,
}

impl<T> CommandViewPage<T> {
    pub fn new(rows: Vec<T>, next: Option<CommandViewCursor>) -> Self {
        Self { rows, next }
    }
    pub fn truncated(&self) -> bool {
        self.next.is_some()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandViewError {
    InvalidLimit,
    StaleCursor,
    RecordTooLarge,
    UnknownWorld,
    UnknownEntity,
}

impl std::fmt::Display for CommandViewError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLimit => "page limit is outside the supported bounds",
            Self::StaleCursor => "cursor does not match this read view",
            Self::RecordTooLarge => "one row exceeds the page byte limit",
            Self::UnknownWorld => "world does not exist",
            Self::UnknownEntity => "entity does not exist in the world",
        })
    }
}

impl std::error::Error for CommandViewError {}

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

#[derive(Clone, Debug)]
pub struct CommandEntityRow {
    pub world: WorldInstanceId,
    pub kind: CommandEntityKind,
}

/// Owned component data for bounded host queries; the reader copies no more
/// than the request byte cap into this result.
#[derive(Clone, Debug)]
pub struct CommandComponentRow {
    pub entity: EntityId,
    pub kind: minimium_world::entity::ComponentTypeId,
    pub schema: minimium_world::entity::SchemaVersion,
    pub bytes: Vec<u8>,
}

/// Component description with its byte count, never its payload.
#[derive(Clone, Copy, Debug)]
pub struct CommandComponentMetadata {
    pub kind: minimium_world::entity::ComponentTypeId,
    pub schema: minimium_world::entity::SchemaVersion,
    pub bytes: usize,
}

#[derive(Clone, Debug)]
pub enum CommandEntityKind {
    Entity(EntityId),
    Player(PlayerId, String),
}

pub trait ModuleCommandReadView {
    fn parse_cursor(&self, token: &str) -> Result<CommandViewCursor, CommandViewError>;
    fn world_count(&self) -> usize;
    fn worlds(
        &self,
        request: &CommandViewRequest,
    ) -> Result<CommandViewPage<CommandWorldView>, CommandViewError>;
    fn world(&self, id: WorldInstanceId) -> Result<CommandWorldView, CommandViewError>;
    fn entities(
        &self,
        world: WorldInstanceId,
        request: &CommandViewRequest,
    ) -> Result<CommandViewPage<EntityId>, CommandViewError>;
    fn players(
        &self,
        request: &CommandViewRequest,
    ) -> Result<CommandViewPage<CommandPlayerView>, CommandViewError>;
    fn entity_rows(
        &self,
        request: &CommandViewRequest,
    ) -> Result<CommandViewPage<CommandEntityRow>, CommandViewError>;
    fn components(
        &self,
        world: WorldInstanceId,
        entity: EntityId,
        request: &CommandViewRequest,
    ) -> Result<CommandViewPage<ComponentView<'_>>, CommandViewError>;

    fn component_metadata(
        &self,
        world: WorldInstanceId,
        entity: EntityId,
        request: &CommandViewRequest,
    ) -> Result<CommandViewPage<CommandComponentMetadata>, CommandViewError> {
        let page = self.components(world, entity, request)?;
        Ok(CommandViewPage::new(
            page.rows
                .into_iter()
                .map(|row| CommandComponentMetadata {
                    kind: row.kind,
                    schema: row.schema,
                    bytes: row.bytes.len(),
                })
                .collect(),
            page.next,
        ))
    }
}
