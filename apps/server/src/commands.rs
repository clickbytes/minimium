mod time;
mod world;

use crate::runtime::schedule::{ScheduleDiagnostics, ServerSchedule, SystemFn};
use minimium_platform::modules::{CommandScope, ModuleRegistry};
use std::sync::OnceLock;

type ModuleHandler = fn(&mut CommandContext<'_>, &[&str]) -> CommandReply;

fn extensions() -> &'static ModuleRegistry<ModuleHandler, SystemFn> {
    static REGISTRY: OnceLock<ModuleRegistry<ModuleHandler, SystemFn>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut registry = ModuleRegistry::default();
        registry
            .install(&world::WorldCommands)
            .expect("built-in world commands register without collisions");
        registry
    })
}

pub fn initialize_modules() {
    let _ = extensions();
}

pub(crate) fn build_schedule() -> ServerSchedule {
    ServerSchedule::from_systems(extensions().systems())
}

fn execute_registered(line: &str, context: &mut CommandContext<'_>) -> CommandReply {
    match extensions().resolve(line) {
        Some((command, args)) if command.definition.scope == CommandScope::ServerAdmin => {
            (command.definition.handler)(context, &args)
        }
        Some(_) => CommandReply {
            success: false,
            text: "command not permitted in the server admin console".into(),
            stop: false,
        },
        None => CommandReply {
            success: false,
            text: "unknown command; use `help`".into(),
            stop: false,
        },
    }
}

use crate::{environment::EnvironmentClock, world_registry::WorldRegistry};
use crate::{
    metrics::RuntimeMetrics,
    storage::{PersistenceWorker, PlayerStore},
};
use minimium_core::time::MonotonicMs;
use minimium_core::world::World;
use minimium_platform::commands::{server_help, ServerCommand};

pub struct CommandContext<'a> {
    pub world: &'a World,
    pub worlds: &'a WorldRegistry,
    pub player_store: &'a mut PlayerStore,
    pub persistence: &'a mut PersistenceWorker,
    pub map_fingerprint: u64,
    pub lanes: usize,
    pub metrics: &'a RuntimeMetrics,
    pub environment_clock: &'a mut EnvironmentClock,
    pub monotonic_ms: MonotonicMs,
    pub schedule_diagnostics: ScheduleDiagnostics,
}

pub struct CommandReply {
    pub success: bool,
    pub text: String,
    pub stop: bool,
}

pub fn execute(command: &ServerCommand, mut context: CommandContext<'_>) -> CommandReply {
    if let ServerCommand::Registered(line) = command {
        return execute_registered(line, &mut context);
    }
    let CommandContext {
        world: _,
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
            text: format!("{} | {}", server_help(), extensions().commands().iter()
                .map(|command| command.definition.usage).collect::<Vec<_>>().join(" | ")),
            stop: false,
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
            }
        }
        ServerCommand::Metrics => CommandReply {
            success: true,
            text: metrics.summary(worlds.total_players()),
            stop: false,
        },
        ServerCommand::Save => {
            let result = environment_clock
                .capture_saved_state(monotonic_ms)
                .and_then(|saved_clock| {
                    persistence
                        .queue_world_save(player_store, worlds, map_fingerprint, saved_clock)
                        .map_err(|error| error.to_string())
                });
            match result {
                Ok(()) => CommandReply {
                    success: true,
                    text: "world save queued; use status to check completion".to_owned(),
                    stop: false,
                },
                Err(error) => CommandReply {
                    success: false,
                    text: format!("world save: {error}"),
                    stop: false,
                },
            }
        }
        ServerCommand::Stop => CommandReply {
            success: true,
            text: "stopping".to_owned(),
            stop: true,
        },
        ServerCommand::Time(command) => time::execute(*command, environment_clock, monotonic_ms),
        ServerCommand::Registered(_) => unreachable!("registered commands dispatched above"),
    }
}
