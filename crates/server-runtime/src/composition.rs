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

/// Process-owned endpoint/status lifecycle. Runtime shutdown invokes these
/// hooks after simulation workers and durable saves have been drained.
pub trait ProcessLifecyclePort: Send {
    fn remove_endpoint(&mut self) -> std::io::Result<()>;
    fn write_shutdown_status(&mut self, result: &std::io::Result<()>) -> std::io::Result<()>;
}

/// Host-owned content endpoint consumed by client admission and drained on shutdown.
pub trait ContentServicePort: Send {
    fn material_hash(&self) -> [u8; 32];
    fn http_port(&self) -> u16;
    fn shutdown(&mut self, timeout: Duration) -> Vec<String>;
}

/// Path-free simulation settings consumed after host bootstrap has completed.
#[derive(Clone)]
pub struct RuntimeSettings {
    pub max_players: usize,
    pub autosave_seconds: u64,
    pub bolid_interval: Option<(u64, u64)>,
    pub resolved_settings:
        std::collections::BTreeMap<String, minimium_commands::ResolvedConfigValue>,
}

/// Validated fixed-step and monotonic scheduling values for impacts/hydrology.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AsyncWorkPolicy {
    impact_fixed_step: Duration,
    impact_checkpoint_age: Duration,
    hydrology_cadence: Duration,
    hydrology_fixed_step: Duration,
    water_checkpoint_cadence: Duration,
    hydrology_retry_delay: Duration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AsyncWorkPolicyError {
    InvalidInterval,
    InvalidFixedStep,
}

impl std::fmt::Display for AsyncWorkPolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for AsyncWorkPolicyError {}

impl AsyncWorkPolicy {
    const MAX_INTERVAL: Duration = Duration::from_secs(604_800);
    const MAX_IMPACT_STEP: Duration = Duration::from_millis(100);
    const MAX_HYDROLOGY_STEP: Duration = Duration::from_secs(1);

    pub fn new(
        impact_fixed_step: Duration,
        impact_checkpoint_age: Duration,
        hydrology_cadence: Duration,
        hydrology_fixed_step: Duration,
        water_checkpoint_cadence: Duration,
        hydrology_retry_delay: Duration,
    ) -> Result<Self, AsyncWorkPolicyError> {
        for interval in [
            impact_checkpoint_age,
            hydrology_cadence,
            water_checkpoint_cadence,
            hydrology_retry_delay,
        ] {
            if interval.is_zero() || interval > Self::MAX_INTERVAL {
                return Err(AsyncWorkPolicyError::InvalidInterval);
            }
        }
        if impact_fixed_step.is_zero() || impact_fixed_step > Self::MAX_IMPACT_STEP {
            return Err(AsyncWorkPolicyError::InvalidFixedStep);
        }
        if hydrology_fixed_step.is_zero() || hydrology_fixed_step > Self::MAX_HYDROLOGY_STEP {
            return Err(AsyncWorkPolicyError::InvalidFixedStep);
        }
        Ok(Self {
            impact_fixed_step,
            impact_checkpoint_age,
            hydrology_cadence,
            hydrology_fixed_step,
            water_checkpoint_cadence,
            hydrology_retry_delay,
        })
    }

    pub const fn impact_fixed_step(self) -> Duration {
        self.impact_fixed_step
    }

    /// Threshold in modeled body age, not host monotonic time.
    pub const fn impact_checkpoint_age(self) -> Duration {
        self.impact_checkpoint_age
    }

    pub const fn hydrology_cadence(self) -> Duration {
        self.hydrology_cadence
    }

    pub const fn hydrology_fixed_step(self) -> Duration {
        self.hydrology_fixed_step
    }

    pub const fn water_checkpoint_cadence(self) -> Duration {
        self.water_checkpoint_cadence
    }

    pub const fn hydrology_retry_delay(self) -> Duration {
        self.hydrology_retry_delay
    }
}

impl Default for AsyncWorkPolicy {
    fn default() -> Self {
        Self {
            impact_fixed_step: Duration::from_millis(50),
            impact_checkpoint_age: Duration::from_secs(5),
            hydrology_cadence: Duration::from_millis(100),
            hydrology_fixed_step: Duration::from_millis(100),
            water_checkpoint_cadence: Duration::from_secs(1),
            hydrology_retry_delay: Duration::from_secs(5),
        }
    }
}

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
    pub terrain_material_profile: minimium_procgen::ResolvedTerrainMaterialProfile,
    pub resolved_relief_policy: Option<minimium_procgen::ResolvedPlanetReliefPolicy>,
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
    pub primary_terrain_material_profile: minimium_procgen::TerrainMaterialProfile,
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
#[derive(Clone, Copy)]
pub struct ExcavationConfig {
    pub terrain: minimium_terrain::TerrainConfig,
    pub tools: minimium_simulation::tools::ToolProfile,
    pub tool_grants: [Option<minimium_world::items::ItemRecord>; 2],
    pub fluids: minimium_fluids::FluidConfig,
    pub server_fluid_cells: usize,
}

impl ExcavationConfig {
    pub fn valid(self) -> bool {
        self.terrain.valid()
            && self.tools.valid()
            && self
                .tool_grants
                .iter()
                .flatten()
                .all(|item| self.tools.definition(item.kind).is_some())
            && (1..=262_144).contains(&self.fluids.max_active_cells)
            && (self.fluids.max_active_cells..=524_288).contains(&self.server_fluid_cells)
    }
}

/// Application-selected world and combat data for one server process.
pub struct ServerComposition {
    pub excavation: ExcavationConfig,
    pub impact_profile: Option<minimium_world::impact::ImpactProfile>,
    pub async_work_policy: AsyncWorkPolicy,
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
    /// Process-host-created socket adapter; domain runtimes may supply a non-TCP host port.
    pub transport: Option<Box<dyn minimium_transport::TransportPort>>,
}
