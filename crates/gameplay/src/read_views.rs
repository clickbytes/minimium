use crate::ComponentView;
use minimium_core::ids::PlayerId;
use minimium_world::{
    entity::EntityId,
    world::{GenerationRevision, WorldInstanceId, WorldManifest, WorldTopology},
};
use std::sync::Arc;

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
        let scope = match self.scope {
            CommandViewScope::None => String::from("n"),
            CommandViewScope::World(world) => format!("w{:x}", world.get()),
            CommandViewScope::Entity { world, entity } => {
                format!("e{:x}-{:x}", world.get(), entity.get())
            }
        };
        format!(
            "v2:{:x}:{}:{}:{:x}:{:x}:{:x}:{:x}",
            Arc::as_ptr(&self.identity) as usize,
            self.query.code(),
            scope,
            self.revision,
            self.secondary_revision,
            self.tertiary_revision,
            self.after
        )
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
        if token.len() > 256 {
            return Err(CommandViewError::StaleCursor);
        }
        let parts = token.split(':').collect::<Vec<_>>();
        if parts.len() != 8
            || parts[0] != "v2"
            || parts[1] != format!("{:x}", Arc::as_ptr(&self.0) as usize)
        {
            return Err(CommandViewError::StaleCursor);
        }
        let query = parts[2]
            .parse::<u8>()
            .ok()
            .and_then(CommandViewKind::from_code)
            .ok_or(CommandViewError::StaleCursor)?;
        let scope = match parts[3].as_bytes().first() {
            Some(b'n') if parts[3] == "n" => CommandViewScope::None,
            Some(b'w') => CommandViewScope::World(
                minimium_world::world::WorldInstanceId::new(
                    u128::from_str_radix(&parts[3][1..], 16)
                        .map_err(|_| CommandViewError::StaleCursor)?,
                )
                .ok_or(CommandViewError::StaleCursor)?,
            ),
            Some(b'e') => {
                let (world, entity) = parts[3][1..]
                    .split_once('-')
                    .ok_or(CommandViewError::StaleCursor)?;
                CommandViewScope::Entity {
                    world: minimium_world::world::WorldInstanceId::new(
                        u128::from_str_radix(world, 16)
                            .map_err(|_| CommandViewError::StaleCursor)?,
                    )
                    .ok_or(CommandViewError::StaleCursor)?,
                    entity: EntityId::new(
                        u64::from_str_radix(entity, 16)
                            .map_err(|_| CommandViewError::StaleCursor)?,
                    )
                    .ok_or(CommandViewError::StaleCursor)?,
                }
            }
            _ => return Err(CommandViewError::StaleCursor),
        };
        let revision =
            u128::from_str_radix(parts[4], 16).map_err(|_| CommandViewError::StaleCursor)?;
        let secondary_revision =
            u128::from_str_radix(parts[5], 16).map_err(|_| CommandViewError::StaleCursor)?;
        let tertiary_revision =
            u128::from_str_radix(parts[6], 16).map_err(|_| CommandViewError::StaleCursor)?;
        let after =
            u128::from_str_radix(parts[7], 16).map_err(|_| CommandViewError::StaleCursor)?;
        Ok(self.issue_versions(
            query,
            scope,
            revision,
            secondary_revision,
            tertiary_revision,
            after,
        ))
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
}
