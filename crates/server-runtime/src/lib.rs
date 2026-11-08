//! Authoritative world lifecycle, ticking, networking and persistence runtime.
mod accept;
mod admin;
pub mod authentication;
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
mod receive;
mod replication;
mod sector_streaming;
mod send;
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
const CHAT_COOLDOWN: Duration = Duration::from_secs(1);
const GLOBAL_CHAT_COOLDOWN: Duration = Duration::from_millis(50);
mod runtime;

pub use commands::{CommandContext as ModuleCommandContext, CommandReply as ModuleCommandReply};
pub use minimium_gameplay::EngineModule;
pub use runtime::schedule::{ModuleSystem, ModuleSystemContext};
pub type ModuleCommandHandler = fn(&mut ModuleCommandContext<'_>, &[&str]) -> ModuleCommandReply;
pub type ServerGameplayModule = dyn EngineModule<ModuleCommandHandler, ModuleSystem>;

pub use config::RuntimeServerConfig;
pub use process_error::{ServerProcessError, ServerProcessErrorKind};

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
    let module_registry = commands::initialize_modules(&composition.modules).map_err(|error| {
        ServerProcessError::caused_by(
            ServerProcessErrorKind::RuntimeSetup,
            std::io::Error::other(error),
        )
    })?;
    let component_registry = commands::component_registry(&module_registry).map_err(|error| {
        ServerProcessError::caused_by(
            ServerProcessErrorKind::RuntimeSetup,
            std::io::Error::other(format!("component registry is invalid: {error:?}")),
        )
    })?;
    let startup = startup::initialize(
        &config,
        composition.world_composer,
        &component_registry,
        composition.storage_logger,
        composition.server_logger.clone(),
    )
    .map_err(|error| ServerProcessError::from_boxed(ServerProcessErrorKind::Startup, error))?;
    let attempt_id = config.startup_attempt_id.clone();
    let run_id = config.diagnostics_run_id.clone();
    let data_dir = config.data_dir.clone();
    let logger = composition.server_logger.clone();
    let runtime = runtime::ServerRuntime::new(
        config,
        startup,
        composition.combat,
        composition.server_logger,
        composition.transport_logger,
        composition.environment_logger,
        composition.protocol_logger,
        composition.world_logger,
        composition.sectors_logger,
        composition.simulation_logger,
        composition.asset_io_logger,
        composition.authentication_provider,
        composition.shutdown_request,
        module_registry,
        component_registry,
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
