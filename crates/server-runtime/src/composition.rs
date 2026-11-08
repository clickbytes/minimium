//! Small host-to-runtime inputs selected by the application composition root.
use minimium_diagnostics::DomainLogger;
use minimium_simulation::CombatSettings;
use minimium_world::world::{SectorKey, WorldInstanceId, WorldManifest, WorldMap};
use std::{error::Error, path::PathBuf, sync::Arc};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};

/// Cloneable process-to-runtime shutdown signal; requesting stop is nonblocking.
#[derive(Default)]
struct ShutdownState {
    requested: AtomicBool,
    log_deadline: Mutex<Option<Instant>>,
}

#[derive(Clone, Default)]
pub struct ShutdownRequest(Arc<ShutdownState>);

impl ShutdownRequest {
    pub fn request(&self) {
        self.0.requested.store(true, Ordering::Release);
    }
    pub fn is_requested(&self) -> bool {
        self.0.requested.load(Ordering::Acquire)
    }

    pub(crate) fn begin_shutdown(&self) -> Instant {
        let mut deadline = self
            .0
            .log_deadline
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *deadline.get_or_insert_with(|| Instant::now() + Duration::from_secs(30))
    }

    /// The process host uses the same absolute deadline for its diagnostics flush.
    pub fn log_deadline(&self) -> Option<Instant> {
        *self
            .0
            .log_deadline
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// The world data the runtime needs to initialize one world instance.
#[derive(Clone, Debug)]
pub struct WorldDefinition {
    pub instance: WorldInstanceId,
    pub manifest: WorldManifest,
    pub map: WorldMap,
    pub initial_entities: Vec<InitialEntity>,
}

#[derive(Clone, Debug)]
pub struct InitialEntity {
    pub sector: SectorKey,
    pub components: Vec<InitialComponent>,
}

#[derive(Clone, Debug)]
pub struct InitialComponent {
    pub kind: minimium_world::entity::ComponentTypeId,
    pub schema: minimium_world::entity::SchemaVersion,
    pub bytes: Vec<u8>,
}

/// Startup inputs that are only available after the runtime reads its data directory.
pub struct WorldCompositionRequest {
    pub data_dir: PathBuf,
    pub requested_seed: Option<u128>,
    pub existing_seed: Option<u128>,
    pub map_path: Option<PathBuf>,
    pub world_definition_path: Option<PathBuf>,
    pub planet_config_path: Option<PathBuf>,
}

/// Sample or application-owned world data returned to the generic server runtime.
pub struct WorldComposition {
    pub seed: u128,
    pub primary_map: WorldMap,
    pub worlds: Option<Vec<WorldDefinition>>,
    pub planet_definition: Option<Vec<u8>>,
    pub prepared_geography: Option<Arc<dyn minimium_procgen::PreparedGeography>>,
}

/// The one application callback used to build world data during server initialization.
pub type WorldComposer =
    Arc<dyn Fn(WorldCompositionRequest) -> Result<WorldComposition, Box<dyn Error>> + Send + Sync>;

/// Existing sample combat values passed across the host/runtime boundary.
#[derive(Clone, Copy)]
pub struct ServerCombatConfig {
    pub hitscan: CombatSettings,
    pub fire_cooldown_ms: u64,
}

/// Application-selected world and combat data for one server process.
#[derive(Clone)]
pub struct ServerComposition {
    pub shutdown_request: ShutdownRequest,
    pub modules: Vec<std::sync::Arc<crate::ServerGameplayModule>>,
    pub world_composer: WorldComposer,
    pub combat: ServerCombatConfig,
    pub server_logger: DomainLogger,
    pub storage_logger: DomainLogger,
    pub transport_logger: DomainLogger,
    pub environment_logger: DomainLogger,
    pub protocol_logger: DomainLogger,
    pub world_logger: DomainLogger,
    pub sectors_logger: DomainLogger,
    pub simulation_logger: DomainLogger,
    pub asset_io_logger: DomainLogger,
    pub authentication_provider: crate::authentication::SharedAuthenticationProvider,
}
