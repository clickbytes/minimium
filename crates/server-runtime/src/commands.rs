mod administration;
pub(crate) mod bolids;
mod generation;
pub(crate) mod water;
pub(crate) use generation::{encode as encode_generation, project as project_generation};
mod configuration;
mod owner_context;
pub use configuration::ConfigurationValue;
pub(crate) use configuration::{
    encode as encode_configuration, project as project_configuration, ConfigurationProjection,
};
mod reply_data;
pub use reply_data::{
    ClientTrafficRow, CommandHelp, EntityComponentRow, EntityKindView, EntityRow, PlayerRow,
    PropertyReport, RuntimeStatusSnapshot, StatusReport, WorldInspect, WorldRow,
};
pub(crate) use reply_data::{ClientTrafficSnapshot, EntityKind, PropertyReportOutcome};
mod time;
mod world;

pub(crate) use owner_context::OwnerCommandContext;
pub(crate) use world::catalog_entry as world_catalog_entry;
pub(crate) use world::RuntimeCommandReadView;

use crate::modules::{CommandScope, ModuleCommandContext, ModuleRegistry, WorldEffectBuffer};
use crate::runtime::schedule::{
    production_schedule, ScheduleDiagnostics, ServerSchedule, SystemFn,
};
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
    registry.reserve_command_names(&crate::server_command::reserved_command_names())?;
    registry.install(&world::WorldCommands)?;
    for module in modules {
        registry.install_mapped(module.as_ref(), ModuleHandler::Extension)?;
    }
    Ok(registry)
}

pub(crate) fn build_schedule(
    registry: &ModuleRegistry<ModuleHandler, SystemFn>,
    definition: &minimium_gameplay::ScheduleDefinition,
) -> Result<ServerSchedule, crate::modules::ScheduleError> {
    ServerSchedule::from_plan(&registry.ordered_schedule(definition)?)
}

pub(crate) fn production_schedule_definition() -> minimium_gameplay::ScheduleDefinition {
    production_schedule()
}

pub(crate) fn component_registry(
    modules: &ModuleRegistry<ModuleHandler, SystemFn>,
) -> Result<minimium_world::entity::ComponentRegistry, minimium_world::entity::ComponentError> {
    let mut registry = minimium_world::entity::ComponentRegistry::default();
    minimium_world::impact::register_component(&mut registry)?;
    for component in modules.components() {
        registry.register_module_component(component.module, component.descriptor)?;
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
    ClockCommitted(crate::environment::ClockPropertySnapshot),
    Property(PropertyReport),
    PropertyHelp(minimium_commands::PropertySpec, String),
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
        revision: Option<u64>,
    },
    Bolid {
        id: u64,
        kind: u8,
        recipients: usize,
        shower: Option<usize>,
        world: minimium_world::world::WorldInstanceId,
        observer: minimium_core::ids::PlayerId,
        context: minimium_protocol::WorldContext,
        tick: u64,
    },
    BolidStatus(crate::runtime::BolidShowerStatus),
    ImpactReceipt(crate::runtime::ImpactReceipt),
    WaterStatus(crate::runtime::WaterStatus),
    ImpactSpawn {
        id: u64,
        world: minimium_world::world::WorldInstanceId,
        observer: minimium_core::ids::PlayerId,
        context: minimium_protocol::WorldContext,
        tick: u64,
    },
    ModuleText(BoundedText),
}

#[derive(Clone, Debug)]
struct CommandFailure {
    subject: Option<BoundedText>,
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
                subject: None,
                code,
                detail: BoundedText::new(detail),
            }),
            action: CommandAction::Continue,
            effects: None,
        }
    }
    pub(crate) fn property_error(code: &'static str, key: &str, reason: &str) -> Self {
        let mut reply = Self::error(code, reason);
        if let Err(error) = &mut reply.outcome {
            error.subject = Some(BoundedText::new(key));
        }
        reply
    }
    pub fn error_subject(&self) -> Option<&str> {
        self.outcome
            .as_ref()
            .err()?
            .subject
            .as_ref()
            .map(BoundedText::as_str)
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
                    if matches!(self.payload(), Some(CommandPayload::EntityDespawn { .. })) {
                        if let Some(crate::modules::WorldEffectOutcome::EntityRejected(reason)) =
                            outcome.results().first()
                        {
                            return world::despawn_rejection_code(*reason);
                        }
                    }
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
    catalog: &minimium_commands::operation::OperationCatalog,
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
        ServerCommand::CatalogOutput { text, .. } => {
            CommandReply::module_text(BoundedText::new(text))
        }
        ServerCommand::Help => CommandReply::success(CommandPayload::Help {
            builtins: crate::server_command::catalog::builtin_usages(catalog),
            modules: registry
                .commands()
                .iter()
                .take(256)
                .map(|command| CommandHelp {
                    usage: command.definition.usage,
                    arguments: command.definition.arguments,
                    description: catalog
                        .resolve(command.definition.name)
                        .expect("installed command")
                        .describe(),
                })
                .collect(),
            truncated: registry.commands().len() > 256,
        }),
        ServerCommand::Status => administration::status(
            worlds,
            lanes,
            metrics,
            schedule_diagnostics,
            persistence,
            runtime_status,
        ),
        ServerCommand::Players { cursor, limit } => {
            administration::players(worlds, cursor.as_deref(), *limit)
        }
        ServerCommand::Metrics => administration::metrics(worlds, metrics, client_traffic),
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
        ServerCommand::Stop => administration::stop(),
        ServerCommand::TimeOfDay {
            minute,
            solar,
            observer,
        } => time::set_day(
            *minute,
            *solar,
            observer.as_ref(),
            worlds,
            environment_clock,
            monotonic_ms,
        ),
        ServerCommand::Time(command) => time::execute(*command, environment_clock, monotonic_ms),
        ServerCommand::Property(_) => CommandReply::error(
            "server.property.wrong_lane",
            "setting command must execute on the server owner lane",
        ),
        ServerCommand::SpawnRandomBolid(_)
        | ServerCommand::SpawnRandomMeteorid
        | ServerCommand::BolidStatus(_)
        | ServerCommand::ImpactReceiptStatus { .. }
        | ServerCommand::WaterStatus(_) => CommandReply::error(
            "server.bolid.wrong_lane",
            "bolid command must execute on the server owner lane",
        ),
        ServerCommand::Registered { .. } | ServerCommand::UnresolvedModule(_) => {
            CommandReply::error(
                "server.command.dispatch_error",
                "registered command used the owner command context",
            )
        }
    }
}
