mod owner_context;
mod time;
mod world;

pub(crate) use owner_context::OwnerCommandContext;
pub(crate) use world::RuntimeCommandReadView;

use crate::modules::{CommandScope, ModuleCommandContext, ModuleRegistry, WorldEffectBuffer};
use crate::runtime::schedule::{ScheduleDiagnostics, ServerSchedule, SystemFn};
pub(crate) use minimium_gameplay::ModuleCommandText as BoundedText;
use std::sync::Arc;

type BuiltinHandler = fn(&mut ModuleCommandContext<'_>, &[&str]) -> CommandReply;

#[derive(Clone, Copy)]
pub(crate) enum ModuleHandler {
    Builtin(BuiltinHandler),
    Extension(minimium_gameplay::ModuleCommandHandler),
}

pub(crate) fn initialize_modules(
    modules: &[Arc<crate::ServerGameplayModule>],
) -> Result<ModuleRegistry<ModuleHandler, SystemFn>, crate::modules::ModuleError> {
    let mut registry = ModuleRegistry::default();
    registry.reserve_command_names(crate::server_command::SERVER_COMMAND_NAMES)?;
    registry.install(&world::WorldCommands)?;
    for module in modules {
        registry.install_mapped(module.as_ref(), ModuleHandler::Extension)?;
    }
    Ok(registry)
}

pub(crate) fn build_schedule(
    registry: &ModuleRegistry<ModuleHandler, SystemFn>,
) -> Result<ServerSchedule, crate::modules::ModuleError> {
    Ok(ServerSchedule::from_plan(&registry.ordered_schedule()?))
}

pub(crate) fn component_registry(
    modules: &ModuleRegistry<ModuleHandler, SystemFn>,
) -> Result<minimium_world::entity::ComponentRegistry, minimium_world::entity::ComponentError> {
    let mut registry = minimium_world::entity::ComponentRegistry::default();
    minimium_world::impact::register_component(&mut registry)?;
    for component in modules.components() {
        registry.register(component.descriptor)?;
    }
    Ok(registry)
}

fn execute_registered(
    line: &str,
    context: &mut ModuleCommandContext<'_>,
    registry: &ModuleRegistry<ModuleHandler, SystemFn>,
) -> CommandReply {
    match registry.resolve(line) {
        Some((command, args)) if command.definition.scope == CommandScope::ServerAdmin => {
            let arguments = args.iter().map(String::as_str).collect::<Vec<_>>();
            match command.definition.handler {
                ModuleHandler::Builtin(handler) => handler(context, &arguments),
                ModuleHandler::Extension(handler) => match handler(context, &arguments) {
                    minimium_gameplay::ModuleCommandResult::Success(text) => {
                        CommandReply::module_text(text)
                    }
                    minimium_gameplay::ModuleCommandResult::Error { code, detail } => {
                        CommandReply::error(code.as_str(), detail.as_str())
                    }
                },
            }
        }
        Some(_) => CommandReply::error(
            "server.command.permission_denied",
            "command not permitted in the server admin console",
        ),
        None => CommandReply::error("server.command.unknown", "unknown command; use `help`"),
    }
}

pub(crate) fn execute_module_command(
    line: &str,
    worlds: &crate::world_registry::WorldRegistry,
    effects: &mut WorldEffectBuffer,
    registry: &ModuleRegistry<ModuleHandler, SystemFn>,
) -> CommandReply {
    let view = world::RuntimeCommandReadView::new(worlds);
    let mut context = ModuleCommandContext::new(&view, effects);
    execute_registered(line, &mut context, registry)
}

use crate::server_command::ServerCommand;

pub struct CommandReply {
    outcome: Result<CommandPayload, CommandFailure>,
    action: CommandAction,
    effects: Option<crate::modules::EffectBatchOutcome>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CommandAction {
    Continue,
    Stop,
}

#[derive(Clone, Debug)]
pub(crate) enum CommandPayload {
    Help {
        builtins: Vec<&'static str>,
        modules: Vec<CommandHelp>,
        truncated: bool,
    },
    Status(StatusReport),
    Players {
        rows: Vec<PlayerRow>,
        truncated: bool,
    },
    Metrics {
        snapshot: crate::metrics::RuntimeMetricsSnapshot,
        clients: Vec<ClientTrafficRow>,
        clients_truncated: bool,
    },
    SaveQueued,
    Stopping,
    Time(crate::environment::ClockPropertySnapshot),
    Property(PropertyReport),
    UnknownProperty(String),
    UnavailableProperty {
        key: String,
        reason: String,
    },
    PropertyHelp(minimium_commands::PropertySpec),
    PropertyCommandsHelp {
        builtins: Vec<&'static str>,
    },
    SingleCommandHelp(CommandHelp),
    PropertyList {
        prefix: Option<String>,
        rows: Vec<minimium_commands::PropertySpec>,
        truncated: bool,
    },
    WorldList {
        rows: Vec<WorldRow>,
        truncated: bool,
    },
    WorldInspect(WorldInspect),
    EntityList {
        rows: Vec<EntityRow>,
        truncated: bool,
    },
    EntityInspect {
        world: minimium_world::world::WorldInstanceId,
        entity: minimium_world::entity::EntityId,
        rows: Vec<EntityComponentRow>,
        truncated: bool,
    },
    EntityDespawn {
        world: minimium_world::world::WorldInstanceId,
        entity: minimium_world::entity::EntityId,
    },
    Bolid {
        id: u64,
        kind: u8,
        recipients: usize,
        shower: Option<usize>,
    },
    ImpactSpawn {
        id: u64,
    },
    ModuleText(BoundedText),
}

#[derive(Clone, Debug)]
struct CommandFailure {
    pub code: &'static str,
    pub detail: BoundedText,
}

#[derive(Clone, Debug)]
pub(crate) struct CommandHelp {
    pub usage: &'static str,
    pub arguments: &'static [minimium_commands::CommandArgumentSpec],
}
#[derive(Clone, Debug)]
pub(crate) struct PlayerRow {
    pub id: minimium_core::ids::PlayerId,
    pub name: String,
    pub world: minimium_world::world::WorldInstanceId,
    pub position: [f32; 3],
}
#[derive(Clone, Debug)]
pub(crate) struct WorldRow {
    pub id: minimium_world::world::WorldInstanceId,
    pub players: usize,
    pub topology: minimium_world::world::WorldTopology,
    pub generation: minimium_world::world::GenerationRevision,
}
#[derive(Clone, Debug)]
pub(crate) struct WorldInspect {
    pub id: minimium_world::world::WorldInstanceId,
    pub players: usize,
    pub manifest: minimium_world::world::WorldManifest,
}
#[derive(Clone, Debug)]
pub(crate) struct EntityRow {
    pub world: minimium_world::world::WorldInstanceId,
    pub kind: EntityKind,
}
#[derive(Clone, Debug)]
pub(crate) struct EntityComponentRow {
    pub kind: minimium_world::entity::ComponentTypeId,
    pub schema: minimium_world::entity::SchemaVersion,
    pub bytes: usize,
}
#[derive(Clone, Debug)]
pub(crate) enum EntityKind {
    Entity(u64),
    Player(minimium_core::ids::PlayerId, String),
}
#[derive(Clone, Debug)]
pub(crate) struct PropertyReport {
    pub key: String,
    pub value: minimium_commands::PropertyValue<String>,
}
pub(crate) enum PropertyReportOutcome {
    Value(PropertyReport),
    Unknown(String),
    Unavailable { key: String, reason: String },
}
#[derive(Clone, Debug)]
pub(crate) struct ClientTrafficRow {
    pub id: minimium_core::ids::PlayerId,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub decoded_frames: u64,
    pub queued_frames: u64,
    pub encoding_us: u64,
    pub connected_ms: u128,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct ClientTrafficSnapshot {
    pub rows: Vec<ClientTrafficRow>,
    pub truncated: bool,
}
#[derive(Clone, Debug)]
pub(crate) struct StatusReport {
    pub players: usize,
    pub lanes: usize,
    pub ticks: u64,
    pub clock_probe_requests: u64,
    pub clock_probe_accepted: u64,
    pub clock_probe_rejected: u64,
    pub environment_sync_messages: u64,
    pub scheduled_world_instances: usize,
    pub startup_system_runs: usize,
    pub persistence: minimium_storage::PersistenceStatus,
    pub runtime: Option<RuntimeStatusSnapshot>,
}
#[derive(Clone, Debug)]
pub(crate) struct RuntimeStatusSnapshot {
    pub game_address: String,
    pub max_players: usize,
    pub tick_ms: u128,
    pub snapshot_ms: u128,
    pub autosave_seconds: u64,
    pub protocol_version: u32,
    pub save_version: u32,
    pub map_fingerprint: u64,
    pub environment_profiles: Vec<(u128, u64, minimium_environment::EnvironmentProfile)>,
    pub environment_profiles_truncated: bool,
    pub impact: crate::runtime::ImpactStatus,
    pub excavation: crate::runtime::ExcavationStatus,
}

impl CommandReply {
    pub(crate) fn success(payload: CommandPayload) -> Self {
        Self {
            outcome: Ok(payload),
            action: CommandAction::Continue,
            effects: None,
        }
    }
    pub(crate) fn stopping(payload: CommandPayload) -> Self {
        Self {
            outcome: Ok(payload),
            action: CommandAction::Stop,
            effects: None,
        }
    }
    pub fn module_text(text: BoundedText) -> Self {
        Self::success(CommandPayload::ModuleText(text))
    }
    /// Construct a typed failure. Detail and diagnostic code have fixed byte bounds.
    pub fn error(code: &'static str, detail: &str) -> Self {
        let code = if code.len() <= 128 {
            code
        } else {
            "server.command.failed"
        };
        Self {
            outcome: Err(CommandFailure {
                code,
                detail: BoundedText::new(detail),
            }),
            action: CommandAction::Continue,
            effects: None,
        }
    }
    pub fn is_success(&self) -> bool {
        self.outcome.is_ok()
            && self
                .effects
                .as_ref()
                .map_or(true, |outcome| !outcome.has_rejections())
    }
    pub fn error_code(&self) -> Option<&'static str> {
        self.outcome.as_ref().err().map(|e| e.code).or_else(|| {
            self.effects
                .as_ref()
                .filter(|outcome| outcome.has_rejections())
                .map(|outcome| {
                    if outcome.has_successes() {
                        "server.command.partial_effects"
                    } else {
                        "server.command.effects_rejected"
                    }
                })
        })
    }
    pub fn detail(&self) -> Option<&str> {
        self.outcome
            .as_ref()
            .err()
            .map(|e| e.detail.as_str())
            .or_else(|| {
                self.effects
                    .as_ref()
                    .filter(|outcome| outcome.has_rejections())
                    .map(|_| "one or more command effects were rejected")
            })
    }
    pub fn should_stop(&self) -> bool {
        self.action == CommandAction::Stop
    }
    pub(crate) fn payload(&self) -> Option<&CommandPayload> {
        self.outcome.as_ref().ok()
    }
    pub(crate) fn payload_mut(&mut self) -> Option<&mut CommandPayload> {
        self.outcome.as_mut().ok()
    }
    pub(crate) fn handler_succeeded(&self) -> bool {
        self.outcome.is_ok()
    }
    pub(crate) fn set_effect_outcome(&mut self, outcome: crate::modules::EffectBatchOutcome) {
        self.effects = Some(outcome);
    }
    pub fn effect_outcome(&self) -> Option<&crate::modules::EffectBatchOutcome> {
        self.effects.as_ref()
    }
}

pub fn execute(
    command: &ServerCommand,
    context: OwnerCommandContext<'_>,
    registry: &ModuleRegistry<ModuleHandler, SystemFn>,
) -> CommandReply {
    let OwnerCommandContext {
        worlds,
        player_store,
        persistence,
        map_fingerprint,
        lanes,
        metrics,
        environment_clock,
        monotonic_ms,
        schedule_diagnostics,
        runtime_status,
        client_traffic,
    } = context;
    match command {
        ServerCommand::Help => CommandReply::success(CommandPayload::Help {
            builtins: crate::server_command::server_help_usages(),
            modules: registry
                .commands()
                .iter()
                .take(256)
                .map(|command| CommandHelp {
                    usage: command.definition.usage,
                    arguments: command.definition.arguments,
                })
                .collect(),
            truncated: registry.commands().len() > 256,
        }),
        ServerCommand::Status => CommandReply::success(CommandPayload::Status(StatusReport {
            players: worlds.total_players(),
            lanes,
            ticks: metrics.ticks,
            clock_probe_requests: metrics.clock_probe_requests,
            clock_probe_accepted: metrics.clock_probe_accepted,
            clock_probe_rejected: metrics.clock_probe_rejected,
            environment_sync_messages: metrics.environment_sync_messages,
            scheduled_world_instances: schedule_diagnostics.world_instance_count,
            startup_system_runs: schedule_diagnostics.startup_system_runs,
            persistence: persistence.status(),
            runtime: runtime_status,
        })),
        ServerCommand::Players => {
            let players = worlds.player_snapshots();
            let truncated = players.len() > 512;
            CommandReply::success(CommandPayload::Players {
                rows: players
                    .into_iter()
                    .take(512)
                    .map(|(world, player)| PlayerRow {
                        id: player.id,
                        name: player.name.to_string(),
                        world,
                        position: player.position,
                    })
                    .collect(),
                truncated,
            })
        }
        ServerCommand::Metrics => CommandReply::success(CommandPayload::Metrics {
            snapshot: metrics.snapshot(worlds.total_players()),
            clients: client_traffic.rows,
            clients_truncated: client_traffic.truncated,
        }),
        ServerCommand::Save => {
            if persistence.has_pending_save() {
                let error = persistence.reject_pending_save();
                return CommandReply::error(error.code(), &error.to_string());
            }
            let result = match environment_clock.capture_saved_state(monotonic_ms) {
                Err(error) => Err((error.code(), error.to_string())),
                Ok(saved_clock) => match worlds.snapshot_for_save(map_fingerprint) {
                    Err(error) => Err(("server.world_snapshot_failed", error.to_string())),
                    Ok(input) => player_store
                        .save_snapshot(input, saved_clock)
                        .and_then(|snapshot| persistence.queue_save(snapshot))
                        .map_err(|error| (error.code(), error.to_string())),
                },
            };
            match result {
                Ok(()) => CommandReply::success(CommandPayload::SaveQueued),
                Err((error_code, error)) => {
                    CommandReply::error(error_code, &format!("world save: {error}"))
                }
            }
        }
        ServerCommand::Stop => CommandReply::stopping(CommandPayload::Stopping),
        ServerCommand::Time(command) => time::execute(*command, environment_clock, monotonic_ms),
        ServerCommand::Property(_) => CommandReply::error(
            "server.property.wrong_lane",
            "setting command must execute on the server owner lane",
        ),
        ServerCommand::SpawnRandomBolid(_) | ServerCommand::SpawnRandomMeteorid => {
            CommandReply::error(
                "server.bolid.wrong_lane",
                "bolid command must execute on the server owner lane",
            )
        }
        ServerCommand::Registered(_) => CommandReply::error(
            "server.command.dispatch_error",
            "registered command used the owner command context",
        ),
    }
}
