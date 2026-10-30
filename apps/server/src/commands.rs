mod time;

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
}

pub struct CommandReply {
    pub success: bool,
    pub text: String,
    pub stop: bool,
}

pub fn execute(command: ServerCommand, context: CommandContext<'_>) -> CommandReply {
    let CommandContext {
        world,
        worlds,
        player_store,
        persistence,
        map_fingerprint,
        lanes,
        metrics,
        environment_clock,
        monotonic_ms,
    } = context;
    match command {
        ServerCommand::Help => CommandReply {
            success: true,
            text: server_help(),
            stop: false,
        },
        ServerCommand::Status => CommandReply {
            success: true,
            text: format!(
                "running players={} lanes={lanes} ticks={} {}",
                world.player_count(),
                metrics.ticks,
                persistence.summary()
            ),
            stop: false,
        },
        ServerCommand::Players => {
            let players = world.snapshot();
            let text = if players.is_empty() {
                "no players".to_owned()
            } else {
                players
                    .iter()
                    .map(|player| {
                        format!(
                            "{}:{}@{:.1},{:.1},{:.1}",
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
            text: metrics.summary(world.player_count()),
            stop: false,
        },
        ServerCommand::Save => {
            match persistence.queue_world_save(player_store, worlds, map_fingerprint) {
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
        ServerCommand::Time(command) => time::execute(command, environment_clock, monotonic_ms),
    }
}
