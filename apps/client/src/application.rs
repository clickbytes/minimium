#[cfg(feature = "desktop")]
mod desktop_ops;
#[cfg(feature = "desktop")]
mod desktop_view;
mod game;
mod messages;
mod settings;
mod step;
mod time;
#[cfg(feature = "desktop")]
mod transitions;
#[cfg(feature = "desktop")]
mod ui;

#[cfg(feature = "desktop")]
pub(crate) use desktop_view::{ActiveSceneReady, TransferPresentationReady};
pub(crate) use game::ClientGameState;
#[cfg(feature = "desktop")]
pub(crate) use game::GameplayState;
pub(crate) use messages::ApplicationMessageError;
#[cfg(feature = "desktop")]
pub(crate) use messages::WorldContextChange;
#[cfg(feature = "desktop")]
pub(crate) use settings::ClientSettings;
pub(crate) use settings::{SettingsStore, StartupSettings};
pub(crate) use step::ClientStepInput;
#[cfg(feature = "desktop")]
pub(crate) use step::PredictionSceneRequest;
#[cfg(feature = "desktop")]
pub(crate) use step::{AppliedSessionEvent, ClientStepDisconnect, PredictionInput};
#[cfg(feature = "desktop")]
pub(crate) use transitions::{
    MapOverviewModel, PendingSectorTransition, WorldTransitionController,
};
#[cfg(feature = "desktop")]
pub(crate) use transitions::{MAX_OVERVIEW_ZOOM, MIN_OVERVIEW_ZOOM};
#[cfg(feature = "desktop")]
pub(crate) use ui::ClientStatus;
#[cfg(feature = "desktop")]
pub(crate) use ui::UiModel;

use minimium_client_runtime::{ClientTime, ClientTimeError, ConnectionController};
use minimium_world::world::{WorldInstanceId, WorldMap};
use std::collections::HashMap;

/// Shared client state owner; presentation resources belong to the host renderer.
pub(crate) struct ClientApplication {
    last_operation_at: ClientTime,
    #[cfg(feature = "desktop")]
    pub(crate) ui: UiModel,
    game: ClientGameState,
    connection: ConnectionController,
    action_executor: ClientActionExecutor,
    pending_transfer: Option<messages::ApplicationTransferPreparation>,
    #[cfg(feature = "desktop")]
    tool_profile: minimium_simulation::tools::ToolProfile,
    #[cfg(feature = "desktop")]
    pub(crate) settings: ClientSettings,
    #[cfg(feature = "desktop")]
    pub(crate) input: crate::input::InputMapper,
    world_catalog: HashMap<WorldInstanceId, WorldMap>,
    #[cfg(feature = "desktop")]
    last_step_at: Option<ClientTime>,
    #[cfg(feature = "desktop")]
    transitions: WorldTransitionController,
    #[cfg(feature = "desktop")]
    overview: MapOverviewModel,
    #[cfg(feature = "desktop")]
    pub(crate) status: ClientStatus,
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
    pub(crate) fn apply_altitude_result(
        &mut self,
        context: minimium_protocol::WorldContext,
        position: Option<minimium_world::world::Position3Mm>,
    ) -> bool {
        if self.game.session_context() != Some(context)
            || self
                .connection
                .session()
                .is_none_or(|session| session.active_world_context() != Some(context))
        {
            return false;
        }
        if let Some(position) = position {
            let prediction = &mut self.game.gameplay.prediction;
            prediction.position_mm = Some(position);
            prediction.character.position = position.0.map(|value| value as f32 / 1000.0);
            prediction.character.velocity = [0.0; 3];
            prediction.character.grounded = false;
            prediction.reset();
            self.game.gameplay.sectors_ready = false;
        }
        true
    }

    pub(crate) fn apply_health_update(&mut self, health: u16) -> bool {
        let Some(context) = self.game.session_context() else {
            return false;
        };
        if self
            .connection
            .session()
            .is_none_or(|session| session.active_world_context() != Some(context))
        {
            return false;
        }
        self.game.gameplay.health = health;
        true
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn new(
        ui: UiModel,
        game: ClientGameState,
        connection: ConnectionController,
        settings: ClientSettings,
        input: crate::input::InputMapper,
        world_catalog: HashMap<WorldInstanceId, WorldMap>,
        transitions: WorldTransitionController,
        overview: MapOverviewModel,
        status: ClientStatus,
        now: ClientTime,
    ) -> Result<Self, ClientTimeError> {
        connection.validate_time(now)?;
        game.gameplay.reconciliation.validate_time(now)?;
        Ok(Self {
            last_operation_at: now,
            ui,
            game,
            connection,
            action_executor: ClientActionExecutor::new(now),
            pending_transfer: None,
            #[cfg(feature = "desktop")]
            tool_profile: minimium_game_sample::DEFAULT_TOOL_PROFILE,
            settings,
            input,
            world_catalog,
            #[cfg(feature = "desktop")]
            last_step_at: None,
            transitions,
            overview,
            status,
        })
    }

    pub(crate) fn new_headless(
        connection: ConnectionController,
        worlds: crate::bootstrap::InitialWorlds,
        settings: HeadlessApplicationSettings,
        now: ClientTime,
    ) -> Result<Self, ClientTimeError> {
        connection.validate_time(now)?;
        let game = ClientGameState::for_initial_world(
            worlds.instance,
            worlds.map.spawn(),
            worlds.world_manifests,
        );
        #[cfg(not(feature = "desktop"))]
        let _ = settings;
        #[cfg(feature = "desktop")]
        let desktop = (
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
        Ok(Self {
            last_operation_at: now,
            #[cfg(feature = "desktop")]
            ui: desktop.0,
            game,
            connection,
            action_executor: ClientActionExecutor::new(now),
            pending_transfer: None,
            #[cfg(feature = "desktop")]
            tool_profile: minimium_game_sample::DEFAULT_TOOL_PROFILE,
            #[cfg(feature = "desktop")]
            settings: desktop.1,
            #[cfg(feature = "desktop")]
            input: desktop.2,
            world_catalog: worlds.world_catalog,
            #[cfg(feature = "desktop")]
            last_step_at: None,
            #[cfg(feature = "desktop")]
            transitions: WorldTransitionController::new(false),
            #[cfg(feature = "desktop")]
            overview: MapOverviewModel::new(2.8),
            #[cfg(feature = "desktop")]
            status: desktop.3,
        })
    }

    pub(crate) fn start_remote(
        &mut self,
        address: String,
        now: ClientTime,
    ) -> Result<(), minimium_client_runtime::connector::StartError> {
        self.accept_time(now)
            .map_err(minimium_client_runtime::connector::StartError::Time)?;
        self.connection.start_remote(address, now)
    }

    pub(crate) fn start_local(
        &mut self,
        config: minimium_client_runtime::LocalServerLaunchConfig,
        now: ClientTime,
    ) -> Result<(), minimium_client_runtime::connector::StartError> {
        self.accept_time(now)
            .map_err(minimium_client_runtime::connector::StartError::Time)?;
        self.connection.start_local(config, now)
    }

    pub(crate) fn cancel_connection_attempt(
        &mut self,
        now: ClientTime,
    ) -> Result<(), ClientTimeError> {
        self.accept_time(now)?;
        self.connection.cancel(now)
    }

    fn poll_connector(
        &mut self,
        now: ClientTime,
    ) -> Result<minimium_client_runtime::ConnectionEvent, ClientTimeError> {
        self.connection.poll_connector(now)
    }

    fn poll_admin_result(
        &mut self,
        now: ClientTime,
    ) -> Result<Option<Result<String, minimium_client_runtime::ControllerError>>, ClientTimeError>
    {
        self.connection.poll_admin_result(now)
    }

    fn poll_session(
        &mut self,
        events: &mut Vec<crate::session::SessionEvent>,
        now: ClientTime,
    ) -> Result<(), ClientTimeError> {
        self.connection.poll_session(events, now)
    }

    pub(crate) fn session(&self) -> Option<&crate::session::ClientSession> {
        self.connection.session()
    }

    pub(crate) fn connection_view(&self) -> &ConnectionController {
        &self.connection
    }

    pub(crate) fn is_connection_busy(&self) -> bool {
        self.connection.busy()
    }

    pub(crate) fn record_diagnostic_failure(
        &mut self,
        code: &'static str,
        error: &impl std::fmt::Display,
    ) {
        self.connection.record_diagnostic_failure(code, error);
    }

    pub(crate) fn report_transfer_host_ready(
        &mut self,
        ticket: minimium_client_runtime::TransferTicket,
        now: ClientTime,
    ) -> Result<(), crate::session::QueueError> {
        self.accept_time(now)
            .map_err(crate::session::QueueError::Time)?;
        if !self.transfer_cpu_content_ready(ticket) {
            return Err(crate::session::QueueError::StaleWorldContext);
        }
        if self
            .connection
            .session()
            .is_some_and(|session| session.host_readiness_requested(ticket))
        {
            return Ok(());
        }
        self.connection.report_transfer_host_ready(ticket, now)
    }

    pub(crate) fn pending_transfer_ticket(
        &self,
    ) -> Option<minimium_client_runtime::TransferTicket> {
        self.pending_transfer.map(|preparation| preparation.ticket)
    }

    pub(crate) fn transfer_cpu_content_ready(
        &self,
        ticket: minimium_client_runtime::TransferTicket,
    ) -> bool {
        let Some(preparation) = self.pending_transfer else {
            return false;
        };
        if preparation.ticket != ticket {
            return false;
        }
        let Some(session) = self.connection.session() else {
            return false;
        };
        let Some(content) = session.resolved_world_content_for_context(ticket.context()) else {
            return false;
        };
        if content.manifest() != preparation.manifest
            || content.map().fingerprint() != preparation.map_fingerprint
        {
            return false;
        }
        let Some(terrain) = session.terrain_inputs_for_context(ticket.context()) else {
            return false;
        };
        if preparation.manifest.planet().is_some() && terrain.source().geography().is_none() {
            return false;
        }
        preparation.manifest.planet().is_none()
            || session
                .water_catalog_for_context(ticket.context())
                .is_some()
    }

    pub(crate) fn finish_until(
        &mut self,
        deadline: std::time::Instant,
        now: ClientTime,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.accept_time(now)?;
        self.connection.finish_until(deadline, now)
    }

    pub(crate) fn request_set_time(
        &mut self,
        minute: u16,
        longitude: f64,
        now: ClientTime,
    ) -> Result<(), minimium_client_runtime::ControllerError> {
        self.accept_controller_time(now)?;
        self.connection.request_set_time(minute, longitude, now)
    }

    pub(crate) fn request_time_scale(
        &mut self,
        value: String,
        now: ClientTime,
    ) -> Result<(), minimium_client_runtime::ControllerError> {
        self.accept_controller_time(now)?;
        self.connection.request_time_scale(value, now)
    }

    pub(crate) fn request_time_mode(
        &mut self,
        mode: &str,
        now: ClientTime,
    ) -> Result<(), minimium_client_runtime::ControllerError> {
        self.accept_controller_time(now)?;
        self.connection.request_time_mode(mode, now)
    }

    pub(crate) fn request_time_offline_policy(
        &mut self,
        policy: &str,
        now: ClientTime,
    ) -> Result<(), minimium_client_runtime::ControllerError> {
        self.accept_controller_time(now)?;
        self.connection.request_time_offline_policy(policy, now)
    }

    pub(crate) fn request_random_bolid(
        &mut self,
        count: u16,
        now: ClientTime,
    ) -> Result<(), minimium_client_runtime::ControllerError> {
        self.accept_controller_time(now)?;
        self.connection.request_random_bolid(count, now)
    }

    pub(crate) fn request_random_meteorid(
        &mut self,
        now: ClientTime,
    ) -> Result<(), minimium_client_runtime::ControllerError> {
        self.accept_controller_time(now)?;
        self.connection.request_random_meteorid(now)
    }

    pub(crate) fn execute_text_command(
        &mut self,
        input: &str,
        source: crate::client_commands::CommandSource,
        fire_direction: [f32; 3],
        player_position: Option<[f32; 3]>,
        now: ClientTime,
    ) -> crate::client_commands::TextCommandResult {
        if let Err(error) = self.accept_time(now) {
            return crate::client_commands::TextCommandResult::QueueRejected {
                kind: QueuedActionKind::Other,
                error: crate::session::QueueError::Time(error),
            };
        }
        let mut context = crate::client_commands::CommandContext {
            now,
            connection: &mut self.connection,
            action_executor: &mut self.action_executor,
            fire_direction,
            player_position,
        };
        crate::client_commands::execute_text(input, source, &mut context)
    }

    pub(crate) fn disconnect_session(
        &mut self,
        now: ClientTime,
    ) -> Result<Option<crate::session::ClientSession>, ClientTimeError> {
        self.accept_time(now)?;
        let session = self.connection.take_session(now)?;
        self.game.set_session_context(None);
        self.pending_transfer = None;
        self.game.gameplay.clear_remote_players();
        self.game.gameplay.health = minimium_simulation::PLAYER_MAX_HEALTH;
        self.game.gameplay.sectors_ready = false;
        self.game.gameplay.collision_index = minimium_simulation::CollisionIndex::default();
        self.game.gameplay.collision_revision =
            self.game.gameplay.collision_revision.wrapping_add(1).max(1);
        #[cfg(feature = "desktop")]
        self.action_executor.clear_input_cooldown();
        #[cfg(feature = "desktop")]
        self.transitions.reset();
        Ok(session)
    }
}
mod actions;
pub(crate) use actions::QueuedActionKind;
pub(crate) use actions::{ClientAction, ClientActionExecutor, LocalActionOutcome};
#[cfg(feature = "desktop")]
pub(crate) use actions::{SuppressionReason, UseTrigger};
