//! Shared application composition for CPU and desktop hosts.
mod worlds;

use crate::options::ClientOptions;
use minimium_client_runtime::ConnectionController;
use minimium_diagnostics::{DomainId, DomainLogger, EventCode, Level};
use minimium_localization::Localizer;
use minimium_platform::app_paths::AppPaths;
use minimium_telemetry::logging::DiagnosticsRuntime;
use std::{error::Error, io, sync::Arc};

pub(crate) use worlds::InitialWorlds;

pub(crate) struct ClientBootstrap {
    pub diagnostics: DiagnosticsRuntime,
    pub logger: DomainLogger,
    pub localizer: Localizer,
    pub paths: AppPaths,
    pub worlds: InitialWorlds,
    pub connection: ClientConnection,
}

pub(crate) enum HostCapabilities {
    CpuOnly,
    #[cfg(feature = "desktop")]
    Desktop {
        compressed_materials: bool,
    },
}

/// Fully configured, idle controller awaiting the host's presentation policy.
pub(crate) struct ClientConnection(ConnectionController);

impl ClientConnection {
    pub fn for_host(self, capabilities: HostCapabilities) -> ConnectionController {
        match capabilities {
            HostCapabilities::CpuOnly => self.0,
            #[cfg(feature = "desktop")]
            HostCapabilities::Desktop {
                compressed_materials,
            } => {
                let mut connection = self.0;
                connection.set_presentation(compressed_materials);
                connection
            }
        }
    }
}

impl ClientBootstrap {
    pub fn new(options: &ClientOptions) -> Result<Self, Box<dyn Error>> {
        let diagnostics = DiagnosticsRuntime::start(&options.resolved_log_dir()?)?;
        let logger = diagnostics.logger(DomainId::ClientRuntime);
        let localizer = Localizer::new(Some(logger.clone())).map_err(io::Error::other)?;
        let worlds = worlds::resolve(
            options,
            &diagnostics.logger(DomainId::AssetIo),
            &diagnostics.logger(DomainId::Procgen),
            &localizer,
            options.locale,
        )?;
        let paths = options.paths(worlds.map.fingerprint())?;
        let profile = minimium_platform::identity_store::load_or_create(&paths.profile_dir)?;
        logger.emit(
            logger
                .event(Level::Info, EventCode::new("client.starting")?)
                .field("initial_world", &worlds.instance.to_string()),
        );
        let earth_package_dir = AppPaths::resolve_path(&options.earth_package_dir)?;
        let mut connection = ConnectionController::new();
        connection.register_replica_schema(
            minimium_game_sample::PORTAL_COMPONENT,
            minimium_game_sample::PORTAL_SCHEMA,
        )?;
        connection.set_authentication(minimium_client_runtime::ClientAuthentication::new(
            "dev.profile",
            profile.as_bytes().to_vec(),
        )?);
        connection.set_diagnostic_logger(logger.clone());
        connection.set_content_jobs(Arc::new(crate::content_jobs::HostContentJobs::new(
            paths.cache_dir.clone(),
            options.cache_limit_bytes,
            Arc::new(crate::world_content::SampleWorldContentResolver::new(
                worlds.world_catalog.clone(),
                worlds.world_manifests.clone(),
                Some(earth_package_dir),
            )),
        )))?;
        Ok(Self {
            diagnostics,
            logger,
            localizer,
            paths,
            worlds,
            connection: ClientConnection(connection),
        })
    }
}
