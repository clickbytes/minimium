//! Small host-to-runtime inputs selected by the application composition root.
use minimium_simulation::CombatSettings;
use minimium_world::world::{WorldInstanceId, WorldManifest, WorldMap};
use std::{error::Error, path::PathBuf, sync::Arc};

/// A neutral portal destination stored on a registered world instance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldPortal {
    pub position: [f32; 3],
    pub target_instance: WorldInstanceId,
    pub destination: [f32; 3],
}

/// The world data the runtime needs to initialize one world instance.
#[derive(Clone, Debug)]
pub struct WorldDefinition {
    pub instance: WorldInstanceId,
    pub manifest: WorldManifest,
    pub map: WorldMap,
    pub portal: Option<WorldPortal>,
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
    pub migration_worlds: Option<Vec<WorldDefinition>>,
    pub planet_definition: Option<Vec<u8>>,
    pub prepared_geography: Option<Arc<dyn minimium_procgen::PreparedGeography>>,
}

/// The one application callback used to build world data during server initialization.
pub type WorldComposer = fn(WorldCompositionRequest) -> Result<WorldComposition, Box<dyn Error>>;

/// Existing sample combat values passed across the host/runtime boundary.
#[derive(Clone, Copy)]
pub struct ServerCombatConfig {
    pub hitscan: CombatSettings,
    pub fire_cooldown_ms: u64,
}

/// Application-selected world and combat data for one server process.
#[derive(Clone, Copy)]
pub struct ServerComposition {
    pub world_composer: WorldComposer,
    pub combat: ServerCombatConfig,
}
