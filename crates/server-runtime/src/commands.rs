mod time;
mod world;

use crate::modules::{CommandScope, ModuleRegistry};
use crate::runtime::schedule::{ScheduleDiagnostics, ServerSchedule, SystemFn};
use std::sync::Arc;

pub type ModuleHandler = fn(&mut CommandContext<'_>, &[&str]) -> CommandReply;

pub(crate) fn initialize_modules(
    modules: &[Arc<crate::ServerGameplayModule>],
) -> Result<ModuleRegistry<ModuleHandler, SystemFn>, crate::modules::ModuleError> {
    let mut registry = ModuleRegistry::default();
    registry.reserve_command_names(crate::server_command::SERVER_COMMAND_NAMES)?;
    registry.install(&world::WorldCommands)?;
    for module in modules {
        registry.install(module.as_ref())?;
    }
    Ok(registry)
}

pub(crate) fn build_schedule(
    registry: &ModuleRegistry<ModuleHandler, SystemFn>,
) -> Result<ServerSchedule, crate::modules::ModuleError> {
    Ok(ServerSchedule::from_systems(&registry.ordered_systems()?))
}

pub(crate) fn component_registry(
    modules: &ModuleRegistry<ModuleHandler, SystemFn>,
) -> Result<minimium_world::entity::ComponentRegistry, minimium_world::entity::ComponentError> {
    let mut registry = minimium_world::entity::ComponentRegistry::default();
    for component in modules.components() {
        registry.register(component.descriptor)?;
    }
    Ok(registry)
}

fn execute_registered(
    line: &str,
    context: &mut CommandContext<'_>,
    registry: &ModuleRegistry<ModuleHandler, SystemFn>,
) -> CommandReply {
    match registry.resolve(line) {
        Some((command, args)) if command.definition.scope == CommandScope::ServerAdmin => {
            let arguments = args.iter().map(String::as_str).collect::<Vec<_>>();
            (command.definition.handler)(context, &arguments)
        }
        Some(_) => CommandReply {
            success: false,
            text: "command not permitted in the server admin console".into(),
            stop: false,
            error_code: Some("server.command.permission_denied"),
        },
        None => CommandReply {
            success: false,
            text: "unknown command; use `help`".into(),
            stop: false,
            error_code: Some("server.command.unknown"),
        },
    }
}

use crate::server_command::{server_help, ServerCommand};
use crate::{environment::EnvironmentClock, world_registry::WorldRegistry};
use crate::{
    metrics::RuntimeMetrics,
    storage::{PersistenceWorker, PlayerStore},
};
use minimium_environment::time::MonotonicMs;

pub struct CommandContext<'a> {
    pub(crate) worlds: &'a WorldRegistry,
    pub(crate) player_store: &'a mut PlayerStore,
    pub(crate) persistence: &'a mut PersistenceWorker,
    pub(crate) map_fingerprint: u64,
    pub(crate) lanes: usize,
    pub(crate) metrics: &'a RuntimeMetrics,
    pub(crate) environment_clock: &'a mut EnvironmentClock,
    pub(crate) monotonic_ms: MonotonicMs,
    pub(crate) schedule_diagnostics: ScheduleDiagnostics,
}

impl CommandContext<'_> {
    pub fn world_count(&self) -> usize {
        self.worlds.iter().count()
    }
}

pub struct CommandReply {
    pub success: bool,
    pub text: String,
    pub stop: bool,
    pub error_code: Option<&'static str>,
}

pub fn execute(
    command: &ServerCommand,
    mut context: CommandContext<'_>,
    registry: &ModuleRegistry<ModuleHandler, SystemFn>,
) -> CommandReply {
    if let ServerCommand::Registered(line) = command {
        return execute_registered(line, &mut context, registry);
    }
    let CommandContext {
        worlds,
        player_store,
        persistence,
        map_fingerprint,
        lanes,
        metrics,
        environment_clock,
        monotonic_ms,
        schedule_diagnostics,
    } = context;
    match command {
        ServerCommand::Help => CommandReply {
            success: true,
            text: format!("{} | {}", server_help(), registry.commands().iter()
                .map(|command| minimium_commands::format_command_help(command.definition.usage, command.definition.arguments)).collect::<Vec<_>>().join(" | ")),
            stop: false,
            error_code: None,
        },
        ServerCommand::Status => CommandReply {
            success: true,
            text: format!(
                "running players={} lanes={lanes} ticks={} clock_probe_requests={} clock_probe_accepted={} clock_probe_rejected={} environment_sync_messages={} scheduled_world_instances={} startup_system_runs={} {}",
                worlds.total_players(),
                metrics.ticks,
                metrics.clock_probe_requests,
                metrics.clock_probe_accepted,
                metrics.clock_probe_rejected,
                metrics.environment_sync_messages,
                schedule_diagnostics.world_instance_count,
                schedule_diagnostics.startup_system_runs,
                persistence.summary()
            ),
            stop: false,
            error_code: None,
        },
        ServerCommand::Players => {
            let players = worlds.player_snapshots();
            let text = if players.is_empty() {
                "no players".to_owned()
            } else {
                players
                    .iter()
                    .map(|(instance_id, player)| {
                        format!(
                            "{}:{}:world={instance_id}@{:.1},{:.1},{:.1}",
                            player.id,
                            player.name,
                            player.position[0],
                            player.position[1],
                            player.position[2]
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("; ")
            };
            CommandReply {
                success: true,
                text,
                stop: false,
                error_code: None,
            }
        }
        ServerCommand::Metrics => CommandReply {
            success: true,
            text: metrics.summary(worlds.total_players()),
            stop: false,
            error_code: None,
        },
        ServerCommand::Save => {
            let result = match environment_clock.capture_saved_state(monotonic_ms) {
                Err(error) => Err((error.code(), error.to_string())),
                Ok(saved_clock) => match worlds.snapshot_for_save(map_fingerprint) {
                    Err(error) => Err(("server.world_snapshot_failed", error.to_string())),
                    Ok(input) => persistence
                        .queue_world_save(player_store, input, saved_clock)
                        .map_err(|error| (error.code(), error.to_string())),
                },
            };
            match result {
                Ok(()) => CommandReply {
                    success: true,
                    text: "world save queued; use status to check completion".to_owned(),
                    stop: false,
                    error_code: None,
                },
                Err((error_code, error)) => CommandReply {
                    success: false,
                    text: format!("world save: {error}"),
                    stop: false,
                    error_code: Some(error_code),
                },
            }
        }
        ServerCommand::Stop => CommandReply {
            success: true,
            text: "stopping".to_owned(),
            stop: true,
            error_code: None,
        },
        ServerCommand::Time(command) => time::execute(*command, environment_clock, monotonic_ms),
        ServerCommand::Property(_) => CommandReply {
            success: false,
            text: "setting command must execute on the server owner lane".into(),
            stop: false,
            error_code: Some("server.property.wrong_lane"),
        },
        ServerCommand::SpawnRandomBolid => CommandReply {
            success: false,
            text: "bolid command must execute on the server owner lane".into(),
            stop: false,
            error_code: Some("server.bolid.wrong_lane"),
        },
        ServerCommand::Registered(_) => unreachable!("registered commands dispatched above"),
    }
}
