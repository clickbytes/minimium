//! Authoritative world lifecycle, ticking, networking and persistence runtime.
mod admin;
pub mod authentication;
mod command_output;
mod commands;
pub mod composition;
mod config;
mod connection;
mod content;
mod diagnostics;
mod environment;
mod executor;
mod lifecycle;
mod metrics;
pub mod modules;
mod process_error;
mod replication;
mod sector_streaming;
mod server_command;
mod simulation;
mod startup;
mod storage;
#[cfg(test)]
mod storage_tests;
mod terminal;
mod world_inputs;
mod world_registry;
use composition::ServerComposition;
use std::time::Duration;

const TICK: Duration = Duration::from_millis(minimium_simulation::SIMULATION_STEP_MILLISECONDS);
const SNAPSHOT_INTERVAL: Duration = Duration::from_millis(100);
const MAX_PENDING_HANDSHAKES: usize = 64;
const MAX_PENDING_WORLD_INPUTS: usize = 4096;
const CLIENT_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const CLIENT_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const CLIENT_AUTHENTICATION_TIMEOUT: Duration = Duration::from_secs(10);
const CLIENT_CONTENT_TIMEOUT: Duration = Duration::from_secs(300);
mod hosting;
mod runtime;
pub use commands::{
    ClientTrafficRow, CommandHelp, EntityComponentRow, EntityKindView, EntityRow, PlayerRow,
    PropertyReport, RuntimeStatusSnapshot, StatusReport, WorldInspect, WorldRow,
};
pub use environment::ClockPropertySnapshot;
pub use hosting::replies::{CommandPayloadView, CommandReplyView, ReplyState};
pub use hosting::{
    CommandExecution, RuntimeCommand, RuntimeComposition, RuntimeExecution, RuntimeLoggers,
    RuntimeModules, RuntimeQuery, RuntimeQueryResult, RuntimeStatus, RuntimeTime, RuntimeUtcError,
    ShutdownOutcome, StepOutcome, SubmissionId, SubmitError,
};
pub use metrics::{MetricField, MetricValue, RuntimeMetricsSnapshot};
pub use runtime::ServerRuntime;
pub use runtime::StartupEffectFailure;
pub use runtime::{ComponentOwnerError, ComponentOwnerPort, ExcavationStatus, ImpactStatus};
pub use startup::PreparedRuntime;

pub use commands::CommandReply as ModuleCommandReply;
pub use minimium_gameplay::{
    CommandComponentRow, CommandEntityKind, CommandEntityRow, CommandPlayerView, CommandViewCursor,
    CommandViewError, CommandViewKind, CommandViewPage, CommandViewRequest, CommandViewScope,
    CommandWorldView, EffectBatchOutcome, EffectProducerId, EffectReceipt, EffectToken,
    WorldEffectOutcome, MAX_COMMAND_VIEW_BYTES, MAX_COMMAND_VIEW_ROWS,
};
pub use minimium_gameplay::{EngineModule, ModuleCommandContext, ModuleCommandHandler};
pub use runtime::schedule::{ModuleSystem, ModuleSystemContext};
pub type ServerGameplayModule = dyn EngineModule<ModuleCommandHandler, ModuleSystem>;

pub use composition::{
    AsyncWorkPolicy, AsyncWorkPolicyError, ContentServicePort, ProcessLifecyclePort,
    RuntimeSettings,
};
pub use config::LaneSelection;
pub use config::RuntimeServerConfig;
pub use environment::EnvironmentConfig as RuntimeEnvironmentConfig;
pub use process_error::{ServerProcessError, ServerProcessErrorKind};

/// Rejects unsupported snapshot and excavation journal versions without writes.
pub fn preflight_persistent_formats(data_dir: &std::path::Path) -> std::io::Result<()> {
    startup::preflight_persistent_formats(data_dir)
}

/// Parses server arguments and runs the runtime with application-owned content.
pub fn run(
    args: &[String],
    process_environment: &std::collections::BTreeMap<String, String>,
    composition: ServerComposition,
) -> Result<(), ServerProcessError> {
    if let Some(output) = config::offline_command_with_environment(args, process_environment)
        .map_err(|error| {
            ServerProcessError::caused_by(ServerProcessErrorKind::Configuration, error)
        })?
    {
        println!("{output}");
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!(
            "{}\nminimium-server config validate|show --config PATH",
            config::help_text()
        );
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--version") {
        println!("minimium-server {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let config = config::ServerConfig::parse_with_environment(args, process_environment).map_err(
        |error| ServerProcessError::caused_by(ServerProcessErrorKind::Configuration, error),
    )?;
    run_with_config(config, composition)
}

/// Runs with application-parsed and application-resolved process paths.
pub fn run_with_config(
    config: RuntimeServerConfig,
    composition: ServerComposition,
) -> Result<(), ServerProcessError> {
    let mut runtime_excavation = composition.excavation;
    runtime_excavation.fluids.max_active_cells = config.fluid_world_cells;
    runtime_excavation.server_fluid_cells = config.fluid_server_cells;
    if !runtime_excavation.valid() {
        return Err(ServerProcessError::caused_by(
            ServerProcessErrorKind::RuntimeSetup,
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid excavation or fluid capacity configuration",
            ),
        ));
    }
    if composition
        .impact_profile
        .is_some_and(|profile| !profile.valid())
    {
        return Err(ServerProcessError::caused_by(
            ServerProcessErrorKind::RuntimeSetup,
            std::io::Error::other("invalid impact configuration"),
        ));
    }
    let modules = RuntimeModules::new(&composition.modules).map_err(|error| {
        ServerProcessError::caused_by(ServerProcessErrorKind::RuntimeSetup, error)
    })?;
    let transport = composition.transport.ok_or_else(|| {
        ServerProcessError::caused_by(
            ServerProcessErrorKind::RuntimeSetup,
            std::io::Error::other("server host did not provide a transport adapter"),
        )
    })?;
    let startup = startup::initialize(
        &config,
        composition.world_composer,
        modules.components(),
        composition.storage_logger,
        composition.server_logger.clone(),
        composition.asset_io_logger.clone(),
        transport,
    )
    .map_err(|error| ServerProcessError::from_boxed(ServerProcessErrorKind::Startup, error))?;
    let attempt_id = config.startup_attempt_id.clone();
    let run_id = config.diagnostics_run_id.clone();
    let data_dir = config.data_dir.clone();
    let logger = composition.server_logger.clone();
    let mut execution = RuntimeExecution::new(config.lanes.count(), config.sector_workers)
        .map_err(|error| {
            ServerProcessError::caused_by(ServerProcessErrorKind::RuntimeSetup, error)
        })?;
    if config.console_enabled {
        println!("Type `help` for server commands.");
        execution.console = Some(terminal::spawn(composition.server_logger.clone()).map_err(
            |error| ServerProcessError::caused_by(ServerProcessErrorKind::RuntimeSetup, error),
        )?);
    }
    let session_nonce = minimium_platform::endpoint::new_random_bytes::<16>()
        .map(minimium_environment::time::SessionNonce::new)
        .map_err(|error| {
            ServerProcessError::caused_by(
                ServerProcessErrorKind::RuntimeSetup,
                environment::EnvironmentClockError::caused_by(
                    environment::EnvironmentClockErrorKind::RandomSource,
                    "could not create server session nonce",
                    error,
                ),
            )
        })?;
    let time = RuntimeTime::system_now();
    let runtime = runtime::ServerRuntime::new(
        RuntimeSettings {
            max_players: config.max_players,
            autosave_seconds: config.autosave_seconds,
            bolid_interval: config.bolid_interval,
            resolved_settings: config.resolved_settings.clone(),
        },
        startup,
        RuntimeComposition {
            combat: composition.combat,
            impacts: composition.impact_profile,
            excavation: runtime_excavation,
            async_work_policy: composition.async_work_policy,
            modules,
            loggers: RuntimeLoggers {
                server: composition.server_logger,
                transport: composition.transport_logger,
                environment: composition.environment_logger,
                protocol: composition.protocol_logger,
                world: composition.world_logger,
                sectors: composition.sectors_logger,
                simulation: composition.simulation_logger,
            },
            authentication: composition.authentication_provider,
            shutdown: composition.shutdown_request,
            session_nonce,
        },
        execution,
        time,
    )
    .map_err(|error| ServerProcessError::caused_by(ServerProcessErrorKind::RuntimeSetup, error))?;
    if let (Some(attempt_id), Some(run_id)) = (attempt_id, run_id) {
        let ready = minimium_platform::startup_status::ServerStartupStatus::new(
            attempt_id,
            minimium_platform::startup_status::StartupState::Ready,
            None,
            Some(run_id),
        )
        .map_err(|error| {
            ServerProcessError::caused_by(ServerProcessErrorKind::StartupStatus, error)
        })?;
        minimium_platform::startup_status::write(&data_dir, &ready).map_err(|error| {
            ServerProcessError::caused_by(ServerProcessErrorKind::StartupStatus, error)
        })?;
    }
    logger.emit(logger.event(
        minimium_diagnostics::Level::Info,
        minimium_diagnostics::EventCode::new("server.ready").map_err(|error| {
            ServerProcessError::caused_by(ServerProcessErrorKind::RuntimeSetup, error)
        })?,
    ));
    runtime
        .run()
        .map_err(|error| ServerProcessError::caused_by(ServerProcessErrorKind::Runtime, error))?;
    Ok(())
}
