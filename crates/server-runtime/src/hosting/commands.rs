use crate::server_command::ServerCommand;
use minimium_environment::TimeCommand;
use minimium_gameplay::{CommandPlayerView, CommandViewError, CommandViewPage, CommandWorldView};
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
        Self(ServerCommand::Players)
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
    pub id: SubmissionId,
    pub reply: crate::ModuleCommandReply,
}

#[derive(Clone, Copy, Debug)]
pub enum RuntimeQuery {
    Status,
    Worlds,
    World(WorldInstanceId),
    Players,
}
pub enum RuntimeQueryResult {
    Status(RuntimeStatus),
    Worlds(CommandViewPage<CommandWorldView>),
    World(Result<CommandWorldView, CommandViewError>),
    Players(CommandViewPage<CommandPlayerView>),
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
    pub result: std::io::Result<()>,
}
