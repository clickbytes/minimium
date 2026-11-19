mod action_receipts;
mod admin;
mod command_input;
mod connection_diagnostics;
pub(crate) use connection_diagnostics::ConnectionEffect;
#[cfg(feature = "desktop")]
mod desktop_ops;
#[cfg(feature = "desktop")]
mod desktop_view;
mod scripting;
mod settings;
#[cfg(feature = "desktop")]
mod transitions;
#[cfg(feature = "desktop")]
mod ui;

#[cfg(feature = "desktop")]
pub(crate) use desktop_view::TransferPresentationReady;
#[cfg(feature = "desktop")]
pub(crate) use settings::ClientSettings;
pub(crate) use settings::{SettingsStore, StartupSettings};
#[cfg(feature = "desktop")]
pub(crate) use transitions::{
    MapOverviewModel, PendingSectorTransition, WorldTransitionController,
};
#[cfg(feature = "desktop")]
pub(crate) use transitions::{MAX_OVERVIEW_ZOOM, MIN_OVERVIEW_ZOOM};
#[cfg(feature = "desktop")]
pub(crate) use ui::{ClientStatus, UiModel};

use minimium_client_runtime::{
    ClientApplicationConfig, ClientApplicationCore, ClientTime, ClientTimeError,
    ConnectionController,
};
#[cfg(feature = "desktop")]
use minimium_world::world::{WorldInstanceId, WorldMap};
#[cfg(feature = "desktop")]
use std::collections::HashMap;

pub(crate) use minimium_client_runtime::QueuedActionKind;
pub(crate) use minimium_client_runtime::{
    ApplicationMessageError, ClientAction, ClientStepInput, ClientStepOutput, LocalActionOutcome,
    PredictionPresentationRequest, PredictionSceneRequest, SuppressionReason,
};
#[cfg(feature = "desktop")]
pub(crate) use minimium_client_runtime::{
    AppliedSessionEvent, ClientStepDisconnect, PredictionInput, UseTrigger, WorldContextChange,
};

/// Host facade: one runtime core plus UI, input and presentation state.
pub(crate) struct ClientApplication {
    core: ClientApplicationCore,
    next_command_id: u64,
    connection_requests: connection_diagnostics::Requests,
    action_receipts: action_receipts::PendingActions,
    policy_output_sources:
        std::collections::BTreeMap<(u64, u64), crate::client_commands::CommandSource>,
    admin_output_source: Option<(
        minimium_commands::admin::Correlation,
        crate::client_commands::CommandSource,
    )>,
    #[cfg(feature = "console-scripting")]
    scripting: crate::scripting::ScriptingHost,
    #[cfg(feature = "desktop")]
    pub(crate) ui: UiModel,
    #[cfg(feature = "desktop")]
    pub(crate) settings: ClientSettings,
    #[cfg(feature = "desktop")]
    pub(crate) input: crate::input::InputMapper,
    #[cfg(feature = "desktop")]
    pub(crate) status: ClientStatus,
    #[cfg(feature = "desktop")]
    transitions: WorldTransitionController,
    #[cfg(feature = "desktop")]
    overview: MapOverviewModel,
}

#[cfg(feature = "desktop")]
pub(crate) struct HeadlessApplicationSettings {
    pub(crate) locale: minimium_localization::Locale,
    pub(crate) local_server_config: minimium_client_runtime::LocalServerLaunchConfig,
    pub(crate) require_earth: bool,
    pub(crate) demo: bool,
    pub(crate) config_file_settings: std::collections::BTreeMap<String, String>,
    pub(crate) startup_settings: StartupSettings,
}

#[cfg(not(feature = "desktop"))]
pub(crate) struct HeadlessApplicationSettings;

impl ClientApplication {
    #[cfg(feature = "desktop")]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        ui: UiModel,
        connection: ConnectionController,
        settings: ClientSettings,
        input: crate::input::InputMapper,
        current_instance: WorldInstanceId,
        world_catalog: HashMap<WorldInstanceId, WorldMap>,
        world_manifests: HashMap<WorldInstanceId, minimium_world::world::WorldManifest>,
        bootstrap_terrain: Option<minimium_procgen::TerrainInputs>,
        initial_collision: Option<minimium_client_runtime::world_readiness::CollisionSnapshot>,
        transitions: WorldTransitionController,
        overview: MapOverviewModel,
        status: ClientStatus,
        now: ClientTime,
    ) -> Result<Self, minimium_client_runtime::ClientApplicationCreateError> {
        let require_earth = settings.require_earth;
        let config = ClientApplicationConfig {
            current_instance,
            world_catalog,
            world_manifests,
            bootstrap_terrain,
            tool_profile: minimium_game_sample::DEFAULT_TOOL_PROFILE,
            require_earth,
        };
        let mut core = ClientApplicationCore::new(connection, config, None, now)?;
        if let Some(snapshot) = initial_collision {
            let _ = core.report_active_scene_ready(snapshot, now);
        }
        Ok(Self {
            core,
            next_command_id: 1,
            connection_requests: Default::default(),
            action_receipts: Default::default(),
            policy_output_sources: Default::default(),
            admin_output_source: None,
            #[cfg(feature = "console-scripting")]
            scripting: Default::default(),
            ui,
            settings,
            input,
            status,
            transitions,
            overview,
        })
    }

    pub(crate) fn new_headless(
        connection: ConnectionController,
        worlds: crate::bootstrap::InitialWorlds,
        settings: HeadlessApplicationSettings,
        now: ClientTime,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let manifest = *worlds
            .world_manifests
            .get(&worlds.instance)
            .ok_or("initial world manifest is unavailable")?;
        let bootstrap_terrain = match crate::world_content::bootstrap_terrain_inputs(
            worlds.instance,
            manifest,
            &worlds.map,
        ) {
            Ok(terrain) => Some(terrain),
            Err(error)
                if error.kind()
                    == minimium_procgen::GenerationErrorKind::Sampling(
                        minimium_procgen::EarthSampleError::PackageUnavailable,
                    ) =>
            {
                None
            }
            Err(error) => return Err(Box::new(error)),
        };
        let config = ClientApplicationConfig {
            current_instance: worlds.instance,
            world_catalog: worlds.world_catalog,
            world_manifests: worlds.world_manifests,
            bootstrap_terrain,
            tool_profile: minimium_game_sample::DEFAULT_TOOL_PROFILE,
            #[cfg(feature = "desktop")]
            require_earth: settings.require_earth,
            #[cfg(not(feature = "desktop"))]
            require_earth: false,
        };
        let core = ClientApplicationCore::new(connection, config, None, now).map_err(|error| {
            std::io::Error::other(format!("client application setup failed: {error:?}"))
        })?;
        #[cfg(not(feature = "desktop"))]
        let _ = settings;
        #[cfg(feature = "desktop")]
        let (ui, settings, input, status) = (
            UiModel {
                console: crate::console::Console::new(),
            },
            ClientSettings::new(
                settings.locale,
                settings.local_server_config,
                settings.require_earth,
                settings.demo,
                settings.config_file_settings,
                SettingsStore::new(&settings.startup_settings),
            ),
            crate::input::InputMapper::default(),
            ClientStatus::ClientText(crate::messages::ClientText::TerminalIntro),
        );
        #[cfg(feature = "desktop")]
        let (transitions, overview) = (
            WorldTransitionController::new(false),
            MapOverviewModel::new(2.8),
        );
        #[cfg(not(feature = "desktop"))]
        let _ = (&core, now);
        Ok(Self {
            core,
            next_command_id: 1,
            connection_requests: Default::default(),
            action_receipts: Default::default(),
            policy_output_sources: Default::default(),
            admin_output_source: None,
            #[cfg(feature = "console-scripting")]
            scripting: Default::default(),
            #[cfg(feature = "desktop")]
            ui,
            #[cfg(feature = "desktop")]
            settings,
            #[cfg(feature = "desktop")]
            input,
            #[cfg(feature = "desktop")]
            status,
            #[cfg(feature = "desktop")]
            transitions,
            #[cfg(feature = "desktop")]
            overview,
        })
    }

    pub(crate) fn session(&self) -> Option<&minimium_client_runtime::ClientSession> {
        self.core.session()
    }

    pub(crate) fn connection_view(&self) -> &ConnectionController {
        self.core.connection_view()
    }

    pub(crate) fn record_diagnostic_failure(
        &self,
        code: &'static str,
        error: &impl std::fmt::Display,
    ) {
        self.core.record_diagnostic_failure(code, error);
    }

    pub(crate) fn start_remote(
        &mut self,
        address: String,
        now: ClientTime,
    ) -> Result<(), minimium_client_runtime::connector::StartError> {
        self.core.start_remote(address, now)
    }

    pub(crate) fn start_local(
        &mut self,
        config: minimium_client_runtime::LocalServerLaunchConfig,
        now: ClientTime,
    ) -> Result<(), minimium_client_runtime::connector::StartError> {
        self.core.start_local(config, now)
    }

    pub(crate) fn cancel_connection_attempt(
        &mut self,
        now: ClientTime,
    ) -> Result<(), ClientTimeError> {
        self.core.cancel_connection_attempt(now)
    }

    pub(crate) fn disconnect_session(
        &mut self,
        now: ClientTime,
    ) -> Result<Option<minimium_client_runtime::ClientSession>, ClientTimeError> {
        let session = self.core.disconnect_session(now)?;
        #[cfg(feature = "desktop")]
        self.transitions.reset();
        self.prepare_bootstrap_terrain(now);
        Ok(session)
    }

    pub(crate) fn finish_until(
        &mut self,
        deadline: std::time::Instant,
        now: ClientTime,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.core.finish_until(deadline, now)
    }

    pub(crate) fn step(
        &mut self,
        input: ClientStepInput,
        candidate_readiness: impl FnMut(
            PredictionSceneRequest<'_>,
        ) -> Result<
            Option<minimium_client_runtime::world_readiness::CollisionSnapshot>,
            String,
        >,
        presentation_ready: Option<
            &mut dyn FnMut(
                &minimium_client_runtime::world_readiness::CollisionSnapshot,
                PredictionPresentationRequest,
            ) -> Result<bool, String>,
        >,
    ) -> Result<ClientStepOutput, ClientTimeError> {
        let now = input.now;
        #[cfg(feature = "console-scripting")]
        self.scripting.pump();
        let output = self
            .core
            .step(input, candidate_readiness, presentation_ready)?;
        if output.disconnect.is_some() {
            #[cfg(feature = "desktop")]
            self.transitions.reset();
            self.prepare_bootstrap_terrain(now);
        }
        Ok(output)
    }

    pub(crate) fn retain_terminal_readiness_interests(
        &self,
        readiness: &mut minimium_client_runtime::world_readiness::WorldReadiness,
    ) -> Result<(), minimium_client_runtime::world_readiness::WorldReadinessError> {
        self.core.retain_terminal_readiness_interests(readiness)
    }

    pub(crate) fn ensure_active_collision(
        &mut self,
        readiness: &mut minimium_client_runtime::world_readiness::WorldReadiness,
        now: ClientTime,
    ) -> Result<Option<minimium_client_runtime::world_readiness::CollisionSnapshot>, String> {
        self.core
            .ensure_active_collision(readiness, now)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn ensure_pending_transfer_collision(
        &mut self,
        readiness: &mut minimium_client_runtime::world_readiness::WorldReadiness,
        now: ClientTime,
    ) -> Result<
        Option<(
            minimium_client_runtime::TransferTicket,
            minimium_client_runtime::world_readiness::CollisionSnapshot,
        )>,
        String,
    > {
        self.core
            .ensure_pending_transfer_collision(readiness, now)
            .map_err(|error| error.to_string())
    }

    pub(crate) fn report_transfer_host_ready(
        &mut self,
        ticket: minimium_client_runtime::TransferTicket,
        snapshot: minimium_client_runtime::world_readiness::CollisionSnapshot,
        now: ClientTime,
    ) -> Result<(), crate::session::QueueError> {
        self.core.report_transfer_host_ready(ticket, snapshot, now)
    }

    pub(crate) fn request_random_bolid(
        &mut self,
        count: u16,
        now: ClientTime,
    ) -> Result<(), minimium_client_runtime::ControllerError> {
        self.core.request_random_bolid(count, now)
    }
    pub(crate) fn request_random_meteorid(
        &mut self,
        now: ClientTime,
    ) -> Result<(), minimium_client_runtime::ControllerError> {
        self.core.request_random_meteorid(now)
    }

    pub(crate) fn request_bolid_shower_status(
        &mut self,
        id: u64,
        now: ClientTime,
    ) -> Result<(), minimium_client_runtime::ControllerError> {
        self.core.request_bolid_shower_status(id, now)
    }

    pub(crate) fn request_impact_receipt_status(
        &mut self,
        world: minimium_world::world::WorldInstanceId,
        id: u64,
        now: ClientTime,
    ) -> Result<(), minimium_client_runtime::ControllerError> {
        self.core.request_impact_receipt_status(world, id, now)
    }

    pub(crate) fn request_persistence_save(
        &mut self,
        now: ClientTime,
    ) -> Result<(), minimium_client_runtime::ControllerError> {
        self.core.request_persistence_save(now)
    }
    pub(crate) fn request_persistence_status(
        &mut self,
        id: Option<u128>,
        now: ClientTime,
    ) -> Result<(), minimium_client_runtime::ControllerError> {
        self.core.request_persistence_status(id, now)
    }

    fn prepare_bootstrap_terrain(&mut self, now: ClientTime) {
        let view = self.core.view();
        let instance = view.world_instance;
        let Some(manifest) = view.manifest else {
            return;
        };
        let Some(map) = self.core.world_catalog().get(&instance) else {
            return;
        };
        let terrain = match crate::world_content::bootstrap_terrain_inputs(instance, manifest, map)
        {
            Ok(terrain) => Some(terrain),
            Err(error)
                if error.kind()
                    == minimium_procgen::GenerationErrorKind::Sampling(
                        minimium_procgen::EarthSampleError::PackageUnavailable,
                    ) =>
            {
                None
            }
            Err(error) => {
                self.core
                    .record_diagnostic_failure(error.code(), &error.to_string());
                None
            }
        };
        if let Err(error) = self.core.provide_bootstrap_terrain(terrain, now) {
            self.core
                .record_diagnostic_failure("client.bootstrap_terrain_rejected", &error);
        }
    }
}
