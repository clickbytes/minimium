mod composition;
mod developer_auth;

#[cfg(feature = "allocation-metrics")]
use minimium_telemetry::allocations;
#[cfg(feature = "allocation-metrics")]
#[global_allocator]
static ALLOCATOR: allocations::CountingAllocator = allocations::CountingAllocator;

fn main() {
    if run().is_err() {
        eprintln!("server process failed; inspect its domain log or startup status");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let working_dir = std::env::current_dir()?;
    let process_environment = capture_server_environment()?;
    let offline = args.first().is_some_and(|arg| arg == "config")
        || args
            .iter()
            .any(|arg| matches!(arg.as_str(), "--help" | "-h" | "--version"));
    if offline {
        let paths = minimium_platform::app_paths::AppPaths::resolve_from(
            &working_dir,
            std::path::Path::new("."),
            std::path::Path::new("."),
            std::path::Path::new("."),
            std::path::Path::new("."),
            option_value(&args, "--log-dir")
                .as_deref()
                .map(std::path::Path::new),
        );
        let mut diagnostics =
            minimium_telemetry::logging::DiagnosticsRuntime::start(&paths.log_dir)?;
        let logger = diagnostics.logger(minimium_diagnostics::DomainId::Platform);
        let server_logger = diagnostics.logger(minimium_diagnostics::DomainId::ServerRuntime);
        let storage_logger = diagnostics.logger(minimium_diagnostics::DomainId::Storage);
        let transport_logger = diagnostics.logger(minimium_diagnostics::DomainId::Transport);
        let game_sample_logger = diagnostics.logger(minimium_diagnostics::DomainId::GameSample);
        let environment_logger = diagnostics.logger(minimium_diagnostics::DomainId::Environment);
        let protocol_logger = diagnostics.logger(minimium_diagnostics::DomainId::Protocol);
        let world_logger = diagnostics.logger(minimium_diagnostics::DomainId::World);
        let sectors_logger = diagnostics.logger(minimium_diagnostics::DomainId::Sectors);
        let simulation_logger = diagnostics.logger(minimium_diagnostics::DomainId::Simulation);
        let asset_io_logger = diagnostics.logger(minimium_diagnostics::DomainId::AssetIo);
        let procgen_logger = diagnostics.logger(minimium_diagnostics::DomainId::Procgen);
        logger.emit(logger.event(
            minimium_diagnostics::Level::Info,
            minimium_diagnostics::EventCode::new("server.cli")?,
        ));
        let result = minimium_server_runtime::run(
            &args,
            &process_environment,
            composition::server_composition(
                false,
                None,
                server_logger,
                storage_logger,
                transport_logger,
                game_sample_logger,
                environment_logger,
                protocol_logger,
                world_logger,
                sectors_logger,
                simulation_logger,
                asset_io_logger,
                procgen_logger,
                minimium_server_runtime::composition::ShutdownRequest::default(),
            ),
        );
        drop(logger);
        let flush_result = diagnostics
            .shutdown_until(std::time::Instant::now() + std::time::Duration::from_secs(2));
        result?;
        flush_result?;
        return Ok(());
    }
    let parent_run_id = option_value(&args, "--parent-run-id");
    let selected_log_dir = option_value(&args, "--log-dir");
    let initial_paths = minimium_platform::app_paths::AppPaths::resolve_from(
        &working_dir,
        std::path::Path::new("."),
        std::path::Path::new("."),
        std::path::Path::new("."),
        std::path::Path::new("."),
        selected_log_dir.as_deref().map(std::path::Path::new),
    );
    let mut diagnostics = minimium_telemetry::logging::DiagnosticsRuntime::start_with_parent(
        &initial_paths.log_dir,
        parent_run_id,
    )?;
    let mut config = match minimium_server_runtime::RuntimeServerConfig::parse_with_environment(
        &args,
        &process_environment,
    ) {
        Ok(config) => config,
        Err(error) => {
            let logger = diagnostics.logger(minimium_diagnostics::DomainId::ServerRuntime);
            logger.emit(
                logger
                    .event(
                        minimium_diagnostics::Level::Error,
                        minimium_diagnostics::EventCode::new("server.configuration_failed")?,
                    )
                    .field("stage", "configuration"),
            );
            if let Some(attempt_id) = option_value(&args, "--startup-attempt-id") {
                let data_dir = option_value(&args, "--data-dir")
                    .or_else(|| process_environment.get("MINIMIUM_SERVER_DATA_DIR").cloned())
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| {
                        std::path::PathBuf::from(minimium_platform::files::DEFAULT_SERVER_DATA_DIR)
                    });
                let paths = minimium_platform::app_paths::AppPaths::resolve_from(
                    &working_dir,
                    &data_dir,
                    &data_dir,
                    &data_dir,
                    &data_dir,
                    None,
                );
                let failed = minimium_platform::startup_status::ServerStartupStatus::new(
                    attempt_id,
                    minimium_platform::startup_status::StartupState::Failed,
                    Some("server.configuration_failed".to_owned()),
                    Some(diagnostics.run_id().to_owned()),
                )?;
                minimium_platform::startup_status::write(&paths.data_dir, &failed)?;
            }
            drop(logger);
            diagnostics
                .shutdown_until(std::time::Instant::now() + std::time::Duration::from_secs(2))?;
            return Err(error.into());
        }
    };
    let earth_package = process_environment
        .get("MINIMIUM_EARTH_PACKAGE")
        .map(std::path::PathBuf::from);
    let earth_package = resolve_server_paths(&mut config, earth_package.as_deref(), &working_dir)?;
    let run_id = diagnostics.run_id().to_owned();
    config.diagnostics_run_id = Some(run_id.clone());
    if let Some(attempt_id) = config.startup_attempt_id.as_ref() {
        let starting = minimium_platform::startup_status::ServerStartupStatus::new(
            attempt_id.clone(),
            minimium_platform::startup_status::StartupState::Starting,
            None,
            Some(run_id.clone()),
        )?;
        minimium_platform::startup_status::write(&config.data_dir, &starting)?;
    }
    let logger = diagnostics.logger(minimium_diagnostics::DomainId::ServerRuntime);
    let storage_logger = diagnostics.logger(minimium_diagnostics::DomainId::Storage);
    let transport_logger = diagnostics.logger(minimium_diagnostics::DomainId::Transport);
    let game_sample_logger = diagnostics.logger(minimium_diagnostics::DomainId::GameSample);
    let environment_logger = diagnostics.logger(minimium_diagnostics::DomainId::Environment);
    let protocol_logger = diagnostics.logger(minimium_diagnostics::DomainId::Protocol);
    let world_logger = diagnostics.logger(minimium_diagnostics::DomainId::World);
    let sectors_logger = diagnostics.logger(minimium_diagnostics::DomainId::Sectors);
    let simulation_logger = diagnostics.logger(minimium_diagnostics::DomainId::Simulation);
    let asset_io_logger = diagnostics.logger(minimium_diagnostics::DomainId::AssetIo);
    let procgen_logger = diagnostics.logger(minimium_diagnostics::DomainId::Procgen);
    let startup_code = minimium_diagnostics::EventCode::new("server.starting")?;
    logger.emit(
        logger
            .event(minimium_diagnostics::Level::Info, startup_code)
            .field("phase", "startup"),
    );
    let data_dir = config.data_dir.clone();
    let attempt_id = config.startup_attempt_id.clone();
    let dev_mode = config.dev_mode;
    let shutdown_request = minimium_server_runtime::composition::ShutdownRequest::default();
    let shutdown_clock = shutdown_request.clone();
    let signal_shutdown = shutdown_request.clone();
    ctrlc::set_handler(move || signal_shutdown.request()).map_err(std::io::Error::other)?;
    let result = minimium_server_runtime::run_with_config(
        config,
        composition::server_composition(
            dev_mode,
            earth_package,
            logger.clone(),
            storage_logger,
            transport_logger,
            game_sample_logger,
            environment_logger,
            protocol_logger,
            world_logger,
            sectors_logger,
            simulation_logger,
            asset_io_logger,
            procgen_logger,
            shutdown_request,
        ),
    );
    if let Err(error) = &result {
        logger.emit(
            logger
                .event(
                    minimium_diagnostics::Level::Error,
                    minimium_diagnostics::EventCode::new("server.process_failed")?,
                )
                .field("stage", &format!("{:?}", error.kind()))
                .field("error_code", error.code())
                .field("reason", &error.to_string()),
        );
        if let Some(attempt_id) = attempt_id {
            let failed = minimium_platform::startup_status::ServerStartupStatus::new(
                attempt_id,
                minimium_platform::startup_status::StartupState::Failed,
                Some(error.code().to_owned()),
                Some(run_id),
            )?;
            minimium_platform::startup_status::write(&data_dir, &failed)?;
        }
    }
    drop(logger);
    let flush_deadline = shutdown_clock
        .log_deadline()
        .unwrap_or_else(|| std::time::Instant::now() + std::time::Duration::from_secs(2));
    let flush_result = diagnostics.shutdown_until(flush_deadline);
    result?;
    flush_result?;
    Ok(())
}

fn capture_server_environment(
) -> Result<std::collections::BTreeMap<String, String>, std::env::VarError> {
    [
        "MINIMIUM_SERVER_THREADS",
        "MINIMIUM_SERVER_SECTOR_WORKERS",
        "MINIMIUM_SERVER_DATA_DIR",
        "MINIMIUM_SERVER_BIND",
        "MINIMIUM_SERVER_GAME_PORT",
        "MINIMIUM_SERVER_MAX_PLAYERS",
        "MINIMIUM_SERVER_AUTOSAVE_INTERVAL",
        "MINIMIUM_SERVER_HEADLESS",
        "MINIMIUM_SERVER_SEED",
        "MINIMIUM_SERVER_PLANET_CONFIG",
        "MINIMIUM_SERVER_CLIMATE_RELIEF_INFLUENCE",
        "MINIMIUM_ADMIN_TOKEN",
        "MINIMIUM_EARTH_PACKAGE",
    ]
    .into_iter()
    .into_iter()
    .filter_map(|name| match std::env::var(name) {
        Ok(value) => Some(Ok((name.to_owned(), value))),
        Err(std::env::VarError::NotPresent) => None,
        Err(error) => Some(Err(error)),
    })
    .collect()
}

fn option_value(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|argument| argument == name)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

fn resolve_server_paths(
    config: &mut minimium_server_runtime::RuntimeServerConfig,
    earth_package: Option<&std::path::Path>,
    working_dir: &std::path::Path,
) -> std::io::Result<Option<std::path::PathBuf>> {
    let mut selected = vec![config.data_dir.clone(), config.log_dir.clone()];
    let mut optional_fields = Vec::new();
    macro_rules! capture_path {
        ($field:ident) => {
            if let Some(path) = &config.$field {
                optional_fields.push(stringify!($field));
                selected.push(path.clone());
            }
        };
    }
    capture_path!(material_package_path);
    capture_path!(climate_scenario_path);
    capture_path!(map_path);
    capture_path!(world_definition_path);
    capture_path!(planet_config_path);
    capture_path!(environment_config_path);
    if let Some(path) = earth_package {
        optional_fields.push("earth_package_dir");
        selected.push(path.to_path_buf());
    }
    let selected_refs = selected
        .iter()
        .map(std::path::PathBuf::as_path)
        .collect::<Vec<_>>();
    let resolved =
        minimium_platform::app_paths::AppPaths::resolve_selected_from(working_dir, &selected_refs);
    config.data_dir = resolved[0].clone();
    config.log_dir = resolved[1].clone();
    let mut resolved_earth_package = None;
    for (name, path) in optional_fields
        .into_iter()
        .zip(resolved.into_iter().skip(2))
    {
        match name {
            "material_package_path" => config.material_package_path = Some(path),
            "climate_scenario_path" => config.climate_scenario_path = Some(path),
            "map_path" => config.map_path = Some(path),
            "world_definition_path" => config.world_definition_path = Some(path),
            "planet_config_path" => config.planet_config_path = Some(path),
            "environment_config_path" => config.environment_config_path = Some(path),
            "earth_package_dir" => resolved_earth_package = Some(path),
            _ => unreachable!("only captured path fields are assigned"),
        }
    }
    Ok(resolved_earth_package)
}
