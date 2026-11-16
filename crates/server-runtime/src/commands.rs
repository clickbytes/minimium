mod owner_context;
mod reply_data;
pub use reply_data::{
    ClientTrafficRow, CommandHelp, EntityComponentRow, EntityKindView, EntityRow, PlayerRow,
    PropertyReport, RuntimeStatusSnapshot, StatusReport, WorldInspect, WorldRow,
};
pub(crate) use reply_data::{ClientTrafficSnapshot, EntityKind, PropertyReportOutcome};
mod time;
mod world;

pub(crate) use owner_context::OwnerCommandContext;
pub(crate) use world::RuntimeCommandReadView;

use crate::modules::{CommandScope, ModuleCommandContext, ModuleRegistry, WorldEffectBuffer};
use crate::runtime::schedule::{ScheduleDiagnostics, ServerSchedule, SystemFn};
use minimium_gameplay::ModuleCommandReadView;
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
    command: &minimium_gameplay::RegisteredCommand<ModuleHandler>,
    args: &[String],
    context: &mut ModuleCommandContext<'_>,
) -> CommandReply {
    if command.definition.scope != CommandScope::ServerAdmin {
        return CommandReply::error(
            "server.command.permission_denied",
            "command not permitted in the server admin console",
        );
    }
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

pub(crate) fn execute_module_command(
    identity: Option<&str>,
    argv: &[String],
    worlds: &crate::world_registry::WorldRegistry,
    effects: &mut WorldEffectBuffer,
    registry: &ModuleRegistry<ModuleHandler, SystemFn>,
    sequences: &mut std::collections::BTreeMap<crate::modules::EffectProducerId, Option<u64>>,
) -> CommandReply {
    let view = world::RuntimeCommandReadView::new(worlds);
    let resolved = match identity {
        Some(identity) => {
            resolve_module_identity(identity, registry).map(|command| (command, argv))
        }
        None => resolve_module_argv(argv, registry),
    };
    match resolved {
        Some((command, args)) => {
            let producer = crate::modules::EffectProducerId::Command {
                module: command.module,
                command: command.definition.name,
            };
            let sequence = sequences.entry(producer).or_insert(Some(0));
            let mut context = ModuleCommandContext::new(&view, effects, producer, sequence);
            execute_registered(command, args, &mut context)
        }
        None => CommandReply::error("server.command.unknown", "unknown command; use `help`"),
    }
}

fn resolve_module_identity<'a>(
    identity: &str,
    registry: &'a ModuleRegistry<ModuleHandler, SystemFn>,
) -> Option<&'a minimium_gameplay::RegisteredCommand<ModuleHandler>> {
    registry.commands().iter().find(|command| {
        command.definition.name == identity || command.definition.aliases.contains(&identity)
    })
}

fn resolve_module_argv<'a>(
    argv: &'a [String],
    registry: &'a ModuleRegistry<ModuleHandler, SystemFn>,
) -> Option<(
    &'a minimium_gameplay::RegisteredCommand<ModuleHandler>,
    &'a [String],
)> {
    registry
        .commands()
        .iter()
        .filter_map(|command| {
            let command_length = std::iter::once(command.definition.name)
                .chain(command.definition.aliases.iter().copied())
                .map(|name| name.split_whitespace().collect::<Vec<_>>())
                .filter(|name| {
                    name.len() <= argv.len()
                        && name
                            .iter()
                            .zip(argv)
                            .all(|(expected, actual)| *expected == actual)
                })
                .map(|name| name.len())
                .max()?;
            Some((command, command_length))
        })
        .max_by_key(|(_, command_length)| *command_length)
        .map(|(command, command_length)| (command, &argv[command_length..]))
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
        next_cursor: Option<String>,
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
        next_cursor: Option<String>,
    },
    WorldInspect(WorldInspect),
    EntityList {
        rows: Vec<EntityRow>,
        truncated: bool,
        next_cursor: Option<String>,
    },
    EntityInspect {
        world: minimium_world::world::WorldInstanceId,
        entity: minimium_world::entity::EntityId,
        rows: Vec<EntityComponentRow>,
        truncated: bool,
        next_cursor: Option<String>,
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
        ServerCommand::Players(cursor) => {
            let view = RuntimeCommandReadView::new(worlds);
            let mut request = minimium_gameplay::CommandViewRequest::new(
                minimium_gameplay::MAX_COMMAND_VIEW_ROWS,
                minimium_gameplay::MAX_COMMAND_VIEW_BYTES,
            )
            .expect("built-in player page limits are valid");
            if let Some(token) = cursor {
                match view.parse_cursor(&token) {
                    Ok(cursor) => request = request.after(cursor),
                    Err(_) => {
                        return CommandReply::error(
                            "server.command.invalid_arguments",
                            "players cursor is invalid or stale",
                        )
                    }
                }
            }
            match view.players(&request) {
                Ok(page) => {
                    let truncated = page.truncated();
                    let next_cursor = page.next.as_ref().map(|cursor| cursor.token());
                    CommandReply::success(CommandPayload::Players {
                        rows: page
                            .rows
                            .into_iter()
                            .map(|player| PlayerRow {
                                id: player.id,
                                name: player.name,
                                world: player.world,
                                position: player.position,
                            })
                            .collect(),
                        truncated,
                        next_cursor,
                    })
                }
                Err(_) => {
                    CommandReply::error("server.command.read_failed", "player page unavailable")
                }
            }
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
        ServerCommand::Registered { .. } | ServerCommand::UnresolvedModule(_) => {
            CommandReply::error(
                "server.command.dispatch_error",
                "registered command used the owner command context",
            )
        }
    }
}
