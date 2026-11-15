use crate::server_command::ServerCommand;
use minimium_gameplay::{CommandPlayerView, CommandViewError, CommandViewPage, CommandWorldView};
use minimium_world::world::WorldInstanceId;

/// Validated, bounded server-administration intent. Hosts holding the runtime
/// are administrators; remote principals still use the existing admission port.
#[derive(Clone, Debug)]
pub struct RuntimeCommand(pub(crate) ServerCommand);
impl RuntimeCommand {
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
