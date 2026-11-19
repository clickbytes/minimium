use crate::server_command::ServerCommand;
use minimium_environment::TimeCommand;
use minimium_gameplay::{
    CommandPlayerView, CommandViewError, CommandViewPage, CommandViewRequest, CommandWorldView,
};
use minimium_world::world::WorldInstanceId;

/// Validated, bounded server-administration intent. Hosts holding the runtime
/// are administrators; remote principals still use the existing admission port.
#[derive(Clone, Debug)]
pub struct RuntimeCommand(pub(crate) ServerCommand);

impl RuntimeCommand {
    pub fn help() -> Self {
        Self(ServerCommand::Help)
    }

    pub fn status() -> Self {
        Self(ServerCommand::Status)
    }

    pub fn players() -> Self {
        Self(ServerCommand::Players {
            cursor: None,
            limit: 64,
        })
    }

    pub fn players_after(
        cursor: impl Into<String>,
    ) -> Result<Self, minimium_commands::CommandError> {
        let cursor = cursor.into();
        if cursor.is_empty() || cursor.len() > 256 || cursor.chars().any(char::is_control) {
            return Err(command_error(
                minimium_commands::CommandErrorCode::InvalidArguments,
                "players list",
                "cursor is outside its bounds",
                "players list [CURSOR]",
            ));
        }
        Ok(Self(ServerCommand::Players {
            cursor: Some(cursor),
            limit: 64,
        }))
    }

    pub fn players_page(
        cursor: Option<String>,
        limit: u16,
    ) -> Result<Self, minimium_commands::CommandError> {
        crate::server_command::players_page(cursor, limit).map(Self)
    }

    pub fn world_instances_page(
        cursor: Option<String>,
        limit: u16,
    ) -> Result<Self, minimium_commands::CommandError> {
        Self::world_page("world list", &[], cursor, limit)
    }

    pub fn world_objects_page(
        cursor: Option<String>,
        limit: u16,
    ) -> Result<Self, minimium_commands::CommandError> {
        Self::world_page("entity list", &[], cursor, limit)
    }

    pub fn world_instance(id: WorldInstanceId) -> Self {
        Self(ServerCommand::Registered {
            identity: "world inspect".into(),
            arguments: vec![id.get().to_string()],
        })
    }

    pub fn world_entity_metadata_page(
        world: WorldInstanceId,
        entity: minimium_world::entity::EntityId,
        cursor: Option<String>,
        limit: u16,
    ) -> Result<Self, minimium_commands::CommandError> {
        Self::world_page(
            "entity inspect",
            &[world.get().to_string(), entity.get().to_string()],
            cursor,
            limit,
        )
    }

    /// Submit one world-scoped effect through the ordinary module command and owner staging path.
    pub fn world_entity_despawn(
        world: WorldInstanceId,
        entity: minimium_world::entity::EntityId,
    ) -> Self {
        Self(ServerCommand::Registered {
            identity: "entity despawn".into(),
            arguments: vec![world.get().to_string(), entity.get().to_string()],
        })
    }

    fn world_page(
        identity: &str,
        ids: &[String],
        cursor: Option<String>,
        limit: u16,
    ) -> Result<Self, minimium_commands::CommandError> {
        if !(1..=256).contains(&limit)
            || cursor.as_ref().is_some_and(|token| {
                token.is_empty() || token.len() > 256 || token.chars().any(char::is_control)
            })
        {
            return Err(command_error(
                minimium_commands::CommandErrorCode::InvalidArguments,
                identity,
                "cursor or limit is outside its bounds",
                "world page(cursor?, limit:U16=64)",
            ));
        }
        let mut arguments = ids.to_vec();
        arguments.push(cursor.unwrap_or_else(|| "none".into()));
        arguments.push(limit.to_string());
        Ok(Self(ServerCommand::Registered {
            identity: identity.into(),
            arguments,
        }))
    }

    pub fn metrics() -> Self {
        Self(ServerCommand::Metrics)
    }

    pub fn save_world() -> Self {
        Self(ServerCommand::Save)
    }

    pub fn stop() -> Self {
        Self(ServerCommand::Stop)
    }

    pub fn spawn_random_bolids(count: u16) -> Result<Self, minimium_commands::CommandError> {
        crate::server_command::validate_bolid_count(Some(count))
            .map(ServerCommand::SpawnRandomBolid)
            .map(Self)
    }

    pub fn bolid_shower_status(id: u64) -> Result<Self, minimium_commands::CommandError> {
        if id == 0 {
            return Err(command_error(
                minimium_commands::CommandErrorCode::InvalidArguments,
                "shower_id",
                "shower ID must be a positive u64",
                "server.sky.bolids.status(shower_id)",
            ));
        }
        Ok(Self(ServerCommand::BolidStatus(id)))
    }

    pub fn impact_receipt_status(
        world: WorldInstanceId,
        id: u64,
    ) -> Result<Self, minimium_commands::CommandError> {
        if id == 0 {
            return Err(command_error(
                minimium_commands::CommandErrorCode::InvalidArguments,
                "body_id",
                "body ID must be a positive u64",
                "world.impacts.status(instance_id, body_id)",
            ));
        }
        Ok(Self(ServerCommand::ImpactReceiptStatus { world, id }))
    }

    pub fn spawn_random_meteorite() -> Self {
        Self(ServerCommand::SpawnRandomMeteorid)
    }

    pub fn time(command: TimeCommand) -> Result<Self, minimium_commands::CommandError> {
        crate::server_command::validate_time_command(command)
            .map(ServerCommand::Time)
            .map(Self)
            .map_err(|reason| {
                command_error(
                    minimium_commands::CommandErrorCode::InvalidValue,
                    "time",
                    reason,
                    minimium_environment::TIME_USAGE,
                )
            })
    }

    pub fn get_property(key: impl Into<String>) -> Result<Self, minimium_commands::CommandError> {
        let request = minimium_commands::PropertyRequest::Get { key: key.into() };
        crate::server_command::validate_property_request(request).map(Self)
    }

    pub fn set_autosave_interval(seconds: u64) -> Result<Self, minimium_commands::CommandError> {
        crate::server_command::set_autosave_interval(seconds).map(Self)
    }

    pub fn reset_autosave_interval() -> Result<Self, minimium_commands::CommandError> {
        let request = minimium_commands::PropertyRequest::Reset {
            key: String::from("server.autosave_interval"),
        };
        crate::server_command::validate_property_request(request).map(Self)
    }

    pub fn set_bolid_interval(
        interval_seconds: Option<(u64, u64)>,
    ) -> Result<Self, minimium_commands::CommandError> {
        crate::server_command::set_bolid_interval(interval_seconds).map(Self)
    }

    pub fn reset_bolid_interval() -> Result<Self, minimium_commands::CommandError> {
        let request = minimium_commands::PropertyRequest::Reset {
            key: String::from("server.sky.bolid_interval"),
        };
        crate::server_command::validate_property_request(request).map(Self)
    }

    pub fn list_properties(
        prefix: Option<String>,
    ) -> Result<Self, minimium_commands::CommandError> {
        let request = minimium_commands::PropertyRequest::List { prefix };
        crate::server_command::validate_property_request(request).map(Self)
    }

    pub fn property_help(target: Option<String>) -> Result<Self, minimium_commands::CommandError> {
        let request = minimium_commands::PropertyRequest::Help { target };
        crate::server_command::validate_property_request(request).map(Self)
    }

    /// Creates a registered module command without passing through the text lexer.
    /// The identity may be a canonical multiword name or one of its registered aliases.
    pub fn module<I, A>(
        identity: impl Into<String>,
        arguments: I,
    ) -> Result<Self, minimium_commands::CommandError>
    where
        I: IntoIterator<Item = A>,
        A: Into<String>,
    {
        let identity = identity.into();
        validate_module_identity(&identity)?;
        let mut owned_arguments = Vec::new();
        let mut total_bytes = identity.len();
        for argument in arguments {
            if owned_arguments.len() == 256 {
                return Err(module_command_error());
            }
            let argument = argument.into();
            total_bytes = total_bytes.saturating_add(argument.len());
            if total_bytes > 512 || argument.chars().any(char::is_control) {
                return Err(module_command_error());
            }
            owned_arguments.push(argument);
        }
        let arguments = owned_arguments;
        validate_module_argv(&identity, &arguments)?;
        Ok(Self(ServerCommand::Registered {
            identity,
            arguments,
        }))
    }

    pub fn parse(line: &str) -> Result<Self, minimium_commands::CommandError> {
        if line.len() > 512 {
            return Err(minimium_commands::CommandError::new(
                minimium_commands::CommandErrorCode::InvalidArguments,
                Some("command"),
                "command exceeds 512 bytes",
                Some("help"),
            ));
        }
        ServerCommand::parse(line).map(Self)
    }
}

fn validate_module_argv(
    identity: &str,
    arguments: &[String],
) -> Result<(), minimium_commands::CommandError> {
    let total_bytes = arguments.iter().fold(identity.len(), |total, argument| {
        total.saturating_add(argument.len())
    });
    if validate_module_identity(identity).is_err()
        || arguments.len() > 256
        || arguments
            .iter()
            .any(|argument| argument.chars().any(char::is_control))
        || total_bytes > 512
    {
        return Err(module_command_error());
    }
    Ok(())
}

fn validate_module_identity(identity: &str) -> Result<(), minimium_commands::CommandError> {
    if identity.trim().is_empty()
        || identity.len() > 128
        || identity.split_whitespace().collect::<Vec<_>>().join(" ") != identity
        || identity.chars().any(char::is_control)
    {
        return Err(module_command_error());
    }
    Ok(())
}

fn module_command_error() -> minimium_commands::CommandError {
    command_error(
        minimium_commands::CommandErrorCode::InvalidArguments,
        "module command",
        "module command identity or arguments exceed their bounds",
        "registered module command",
    )
}

fn command_error(
    code: minimium_commands::CommandErrorCode,
    subject: &str,
    reason: &str,
    usage: &str,
) -> minimium_commands::CommandError {
    minimium_commands::CommandError::new(code, Some(subject), reason, Some(usage))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubmissionId(pub(crate) u64);
impl SubmissionId {
    pub fn get(self) -> u64 {
        self.0
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmitError {
    Stopping,
    QueueFull,
    IdentifierExhausted,
}
impl std::fmt::Display for SubmitError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{self:?}")
    }
}
impl std::error::Error for SubmitError {}
pub struct CommandExecution {
    pub identity: minimium_commands::results::OperationContext,
    pub id: SubmissionId,
    pub reply: crate::ModuleCommandReply,
}

impl CommandExecution {
    pub fn operation_result(
        &self,
    ) -> minimium_commands::results::OperationResult<crate::hosting::replies::CommandReplyView<'_>>
    {
        self.reply.view().operation_result(self.identity.clone())
    }
}

#[derive(Clone, Debug)]
pub enum RuntimeQuery {
    Status,
    Worlds,
    WorldsPage(CommandViewRequest),
    World(WorldInstanceId),
    Players,
    PlayersPage(CommandViewRequest),
    Entities(WorldInstanceId, CommandViewRequest),
    EntityRows(CommandViewRequest),
    Components(
        WorldInstanceId,
        minimium_world::entity::EntityId,
        CommandViewRequest,
    ),
    ComponentMetadata(
        WorldInstanceId,
        minimium_world::entity::EntityId,
        CommandViewRequest,
    ),
}
pub enum RuntimeQueryResult {
    Status(RuntimeStatus),
    Worlds(CommandViewPage<CommandWorldView>),
    WorldsPage(Result<CommandViewPage<CommandWorldView>, CommandViewError>),
    World(Result<CommandWorldView, CommandViewError>),
    Players(CommandViewPage<CommandPlayerView>),
    PlayersPage(Result<CommandViewPage<CommandPlayerView>, CommandViewError>),
    Entities(Result<CommandViewPage<minimium_world::entity::EntityId>, CommandViewError>),
    EntityRows(Result<CommandViewPage<minimium_gameplay::CommandEntityRow>, CommandViewError>),
    Components(Result<CommandViewPage<minimium_gameplay::CommandComponentRow>, CommandViewError>),
    ComponentMetadata(
        Result<CommandViewPage<minimium_gameplay::CommandComponentMetadata>, CommandViewError>,
    ),
}
#[derive(Clone, Debug)]
pub struct RuntimeStatus {
    pub simulation_tick: u64,
    pub world_count: usize,
    pub queued_commands: usize,
    pub stopping: bool,
    pub persistence: minimium_storage::PersistenceStatus,
}
pub struct StepOutcome {
    pub simulation_tick: u64,
    pub tick_executed: bool,
    pub stopping: bool,
    pub next_wake: std::time::Instant,
    /// At most eight executed commands, or up to 64 cancellation results on stop.
    pub commands: Vec<CommandExecution>,
}
pub struct ShutdownOutcome {
    pub commands: Vec<CommandExecution>,
    /// Receipts not drained by their producer before shutdown.
    pub effect_receipts: Vec<minimium_gameplay::EffectReceipt>,
    pub result: std::io::Result<()>,
}
