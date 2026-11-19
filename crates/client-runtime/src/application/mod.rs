mod actions;
mod game;
mod messages;
mod ops;
mod readiness;
mod step;
mod time;
mod world_readiness;

pub use actions::{
    ClientAction, LocalActionOutcome, QueuedActionKind, SuppressionReason, UseTrigger,
};
pub use messages::{ApplicationMessageError, WorldContextChange};
pub use step::{
    AppliedSessionEvent, ClientStepDisconnect, ClientStepInput, ClientStepOutput, PredictionInput,
    PredictionPresentationRequest, PredictionSceneRequest,
};

use game::ClientGameState;
use minimium_world::world::{WorldInstanceId, WorldManifest, WorldMap};
use std::collections::HashMap;

/// Prepared CPU world inputs supplied by the application composition root.
/// Maps and manifests are already resolved; no sample-game defaults are read here.
pub struct ClientApplicationConfig {
    pub current_instance: WorldInstanceId,
    pub world_catalog: HashMap<WorldInstanceId, WorldMap>,
    pub world_manifests: HashMap<WorldInstanceId, WorldManifest>,
    pub bootstrap_terrain: Option<minimium_procgen::TerrainInputs>,
    pub tool_profile: minimium_simulation::tools::ToolProfile,
    pub require_earth: bool,
}

#[derive(Debug)]
pub enum ClientApplicationCreateError {
    Time(crate::ClientTimeError),
    InvalidToolProfile,
    MissingInitialWorld,
    MismatchedBootstrapTerrain,
    PreparedSession(crate::PreparedSessionError),
}

#[derive(Debug)]
pub enum ClientReadinessError {
    Time(crate::ClientTimeError),
    Worker(crate::world_readiness::WorldReadinessError),
    MissingManifest,
    MissingMap,
}

impl std::fmt::Display for ClientReadinessError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Time(error) => error.fmt(formatter),
            Self::Worker(error) => error.fmt(formatter),
            Self::MissingManifest => formatter.write_str("active world manifest is unavailable"),
            Self::MissingMap => formatter.write_str("active world map is unavailable"),
        }
    }
}

impl std::error::Error for ClientReadinessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Time(error) => Some(error),
            Self::Worker(error) => Some(error),
            Self::MissingManifest | Self::MissingMap => None,
        }
    }
}

impl std::fmt::Display for ClientApplicationCreateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "client application creation failed: {self:?}")
    }
}

impl std::error::Error for ClientApplicationCreateError {}

impl From<crate::ClientTimeError> for ClientApplicationCreateError {
    fn from(value: crate::ClientTimeError) -> Self {
        Self::Time(value)
    }
}

/// One GPU-independent owner for client session, world, prediction and actions.
pub struct ClientApplicationCore {
    last_operation_at: crate::ClientTime,
    motion_stream: step::MotionStream,
    last_step_at: Option<crate::ClientTime>,
    game: ClientGameState,
    connection: crate::ConnectionController,
    action_executor: actions::ClientActionExecutor,
    pending_transfer: Option<messages::ApplicationTransferPreparation>,
    world_catalog: HashMap<WorldInstanceId, WorldMap>,
    bootstrap_terrain: Option<minimium_procgen::TerrainInputs>,
    tool_profile: minimium_simulation::tools::ToolProfile,
    require_earth: bool,
    map_teleport_pending: bool,
}

impl ClientApplicationCore {
    pub fn new(
        mut connection: crate::ConnectionController,
        config: ClientApplicationConfig,
        prepared_session: Option<crate::ClientSession>,
        now: crate::ClientTime,
    ) -> Result<Self, ClientApplicationCreateError> {
        connection.validate_time(now)?;
        if !config.tool_profile.valid() {
            return Err(ClientApplicationCreateError::InvalidToolProfile);
        }
        if !config.world_catalog.contains_key(&config.current_instance)
            || !config
                .world_manifests
                .contains_key(&config.current_instance)
        {
            return Err(ClientApplicationCreateError::MissingInitialWorld);
        }
        let spawn = config.world_catalog[&config.current_instance].spawn();
        let active_manifest = config.world_manifests[&config.current_instance];
        if let Some(terrain) = &config.bootstrap_terrain {
            let map = &config.world_catalog[&config.current_instance];
            if !terrain
                .source()
                .matches_map(config.current_instance, active_manifest, map)
                || terrain.overlays().world() != Some(config.current_instance)
            {
                return Err(ClientApplicationCreateError::MismatchedBootstrapTerrain);
            }
        }
        if prepared_session.is_none()
            && connection.session().is_some_and(|session| {
                session.state() != crate::SessionState::AwaitingWelcome
                    || session.active_world_context().is_some()
            })
        {
            return Err(ClientApplicationCreateError::PreparedSession(
                crate::PreparedSessionError::InvalidState,
            ));
        }
        let game = ClientGameState::for_initial_world(
            config.current_instance,
            spawn,
            config.world_manifests,
        );
        if let Some(session) = prepared_session {
            connection
                .install_prepared_session(session, now)
                .map_err(ClientApplicationCreateError::PreparedSession)?;
        }
        Ok(Self {
            last_operation_at: now,
            motion_stream: Default::default(),
            last_step_at: None,
            game,
            connection,
            action_executor: actions::ClientActionExecutor::new(now),
            pending_transfer: None,
            world_catalog: config.world_catalog,
            bootstrap_terrain: config.bootstrap_terrain,
            tool_profile: config.tool_profile,
            require_earth: config.require_earth,
            map_teleport_pending: false,
        })
    }

    pub fn view(&self) -> crate::ClientView<'_> {
        self.game.view()
    }

    pub fn session_context(&self) -> Option<minimium_protocol::WorldContext> {
        self.game.session_context()
    }

    pub fn world_catalog(&self) -> &HashMap<WorldInstanceId, WorldMap> {
        &self.world_catalog
    }

    pub fn world_manifest_catalog(&self) -> &HashMap<WorldInstanceId, WorldManifest> {
        &self.game.world_manifests
    }

    pub fn resolved_map_for_context(
        &self,
        context: minimium_protocol::WorldContext,
        fingerprint: u64,
        manifest: WorldManifest,
    ) -> Option<std::sync::Arc<WorldMap>> {
        let content = self
            .connection
            .session()?
            .resolved_world_content_for_context(context)?;
        (content.manifest() == manifest && content.map().fingerprint() == fingerprint)
            .then(|| content.map().clone())
    }

    pub fn session(&self) -> Option<&crate::ClientSession> {
        self.connection.session()
    }

    pub fn connection_view(&self) -> &crate::ConnectionController {
        &self.connection
    }

    pub fn tool_profile(&self) -> minimium_simulation::tools::ToolProfile {
        self.tool_profile
    }

    pub fn submerged_fraction(&self) -> f32 {
        self.game.gameplay.prediction.submerged_fraction()
    }

    /// Read-only tool preview against the installed CPU collision image.
    pub fn preview_tool_surface_hit(
        &self,
        origin: [f64; 3],
        direction: [f64; 3],
        reach_m: f64,
    ) -> Option<minimium_world::terrain::SurfaceHit> {
        let gravity = self.game.gameplay.prediction.character.gravity;
        let blockers = self
            .game
            .gameplay
            .reconciliation
            .tracks()
            .iter()
            .flatten()
            .filter_map(|track| {
                let feet = track.displayed_position_mm.map(|v| v as f64 / 1000.0);
                minimium_world::world::CharacterGeometry::new(feet, gravity)
                    .map(|geometry| minimium_simulation::ToolRayCapsule { geometry })
            });
        self.game.gameplay.collision_index.tool_surface_hit(
            self.game.current_instance,
            origin,
            direction,
            reach_m,
            blockers,
        )
    }

    pub fn scene_terrain_inputs(
        &self,
    ) -> Result<Option<minimium_procgen::TerrainInputs>, minimium_procgen::GenerationError> {
        if let Some(session) = self.connection.session() {
            return Ok(session.terrain_inputs());
        }
        let instance = self.game.current_instance;
        let Some(manifest) = self.game.world_manifests.get(&instance).copied() else {
            return Ok(None);
        };
        let Some(map) = self.world_catalog.get(&instance) else {
            return Ok(None);
        };
        Ok(self
            .bootstrap_terrain
            .as_ref()
            .filter(|terrain| {
                terrain.source().matches_map(instance, manifest, map)
                    && terrain.overlays().world() == Some(instance)
            })
            .cloned())
    }
}
