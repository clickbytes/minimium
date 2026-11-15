#[cfg(feature = "desktop")]
mod desktop_view;
mod game;
mod messages;
mod settings;
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

use minimium_client_runtime::ConnectionController;
use minimium_world::world::{WorldInstanceId, WorldMap};
use std::collections::HashMap;

/// Shared client state owner; presentation resources belong to the host renderer.
pub(crate) struct ClientApplication {
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
    pub(crate) fn game_view(&self) -> &ClientGameState {
        &self.game
    }

    pub(crate) fn world_catalog_view(&self) -> &HashMap<WorldInstanceId, WorldMap> {
        &self.world_catalog
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn resolved_map_for_context(
        &self,
        context: minimium_protocol::WorldContext,
        fingerprint: u64,
        manifest: minimium_world::world::WorldManifest,
    ) -> Option<std::sync::Arc<WorldMap>> {
        let content = self
            .connection
            .session()?
            .resolved_world_content_for_context(context)?;
        (content.manifest == manifest && content.map.fingerprint() == fingerprint)
            .then(|| content.map.clone())
    }

    pub(crate) fn execute_action(&mut self, action: ClientAction) -> LocalActionOutcome {
        self.action_executor.execute(&mut self.connection, action)
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn clear_remote_players(&mut self) {
        self.game.gameplay.clear_remote_players();
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn interpolate_remote_players(
        &mut self,
        now: std::time::Instant,
        delay: std::time::Duration,
    ) -> bool {
        self.game.gameplay.reconciliation.interpolate_positions(
            now,
            delay,
            &mut self.game.gameplay.remote_positions_mm,
        )
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn invalidate_active_scene(
        &mut self,
        context: Option<minimium_protocol::WorldContext>,
        instance: WorldInstanceId,
    ) {
        if self.game.session_context() == context && self.game.current_instance == instance {
            self.game.gameplay.sectors_ready = false;
        }
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn clear_action_input_cooldown(&mut self) {
        self.action_executor.clear_input_cooldown();
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn refresh_prediction_water_read(&mut self) {
        let water = self
            .connection
            .session()
            .and_then(|session| session.water_read());
        self.game.gameplay.prediction.set_water_read(water);
    }

    pub(crate) fn report_sky_view(
        &mut self,
        context: minimium_protocol::WorldContext,
        direction: [f32; 3],
        night: bool,
    ) -> bool {
        if self.game.session_context() != Some(context) {
            return false;
        }
        self.connection.session_mut().is_some_and(|session| {
            session
                .queue(&minimium_protocol::ClientMessage::SkyView {
                    context,
                    direction,
                    night,
                })
                .is_ok()
        })
    }

    pub(crate) fn cancel_transition_request(&mut self) {
        self.connection.cancel_transition();
    }

    pub(crate) fn request_pending_transition_cancel(&mut self) -> bool {
        self.connection
            .session_mut()
            .is_some_and(|session| session.cancel_pending_transition())
    }

    #[cfg(feature = "desktop")]
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

    #[cfg(feature = "desktop")]
    pub(crate) fn apply_map_teleport_position(
        &mut self,
        position: Option<minimium_world::world::Position3Mm>,
    ) -> bool {
        let Some(context) = self.game.session_context() else {
            return false;
        };
        if !self.transitions.map_teleport_pending()
            || self
                .connection
                .session()
                .is_none_or(|session| session.active_world_context() != Some(context))
        {
            return false;
        }
        self.transitions.resolve_map_teleport();
        if let Some(position) = position {
            self.game.gameplay.prediction.reset();
            self.game.gameplay.prediction.position_mm = Some(position);
            self.game.gameplay.prediction.character.position =
                position.0.map(|coordinate| coordinate as f32 / 1000.0);
            self.game.gameplay.prediction.character.velocity = [0.0; 3];
            self.game.gameplay.prediction.character.grounded = false;
            self.game.gameplay.prediction.reset();
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
    pub(crate) fn accumulate_prediction_time(&mut self, delta_seconds: f32) {
        self.game.accumulate_prediction_time(delta_seconds);
    }

    #[cfg(feature = "desktop")]
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn predict_next(
        &mut self,
        direction: [f32; 2],
        vertical: f32,
        jump_pending: bool,
        movement_enabled: bool,
    ) -> Option<minimium_client_runtime::PredictedStep> {
        let manifest = self
            .game
            .world_manifests
            .get(&self.game.current_instance)
            .copied();
        let movement = self.connection.movement;
        let geography = self
            .connection
            .session()
            .and_then(|session| session.prepared_geography());
        self.game.predict_next(
            direction,
            vertical,
            movement,
            jump_pending,
            movement_enabled,
            manifest,
            geography,
        )
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn rollback_prediction(&mut self, step: &minimium_client_runtime::PredictedStep) {
        self.game.gameplay.prediction.character = step.previous_state;
        self.game.gameplay.prediction.position_mm = step.previous_mm;
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn take_prediction_visual_offset(&mut self, delta_seconds: f32) -> [f32; 3] {
        self.game
            .gameplay
            .prediction
            .take_visual_offset(delta_seconds)
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn record_movement_input(&mut self, sample: minimium_protocol::MovementSample) {
        let policy = self.connection.movement;
        let collision_revision = self.game.gameplay.collision_revision;
        self.game
            .gameplay
            .prediction
            .record_input(sample, policy, collision_revision);
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
    ) -> Self {
        Self {
            ui,
            game,
            connection,
            action_executor: ClientActionExecutor::default(),
            pending_transfer: None,
            #[cfg(feature = "desktop")]
            tool_profile: minimium_game_sample::DEFAULT_TOOL_PROFILE,
            settings,
            input,
            world_catalog,
            transitions,
            overview,
            status,
        }
    }

    pub(crate) fn new_headless(
        connection: ConnectionController,
        worlds: crate::bootstrap::InitialWorlds,
        settings: HeadlessApplicationSettings,
    ) -> Self {
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
                session_events: Vec::with_capacity(16),
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
        Self {
            #[cfg(feature = "desktop")]
            ui: desktop.0,
            game,
            connection,
            action_executor: ClientActionExecutor::default(),
            pending_transfer: None,
            #[cfg(feature = "desktop")]
            tool_profile: minimium_game_sample::DEFAULT_TOOL_PROFILE,
            #[cfg(feature = "desktop")]
            settings: desktop.1,
            #[cfg(feature = "desktop")]
            input: desktop.2,
            world_catalog: worlds.world_catalog,
            #[cfg(feature = "desktop")]
            transitions: WorldTransitionController::new(false),
            #[cfg(feature = "desktop")]
            overview: MapOverviewModel::new(2.8),
            #[cfg(feature = "desktop")]
            status: desktop.3,
        }
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn overview(&self) -> &MapOverviewModel {
        &self.overview
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn overview_open(&mut self, instance: WorldInstanceId) {
        self.overview.open(instance);
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn overview_close(&mut self) {
        self.overview.close();
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn overview_set_zoom(&mut self, zoom: f32) {
        self.overview.set_zoom(zoom);
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn overview_begin_slider_drag(&mut self) {
        self.overview.begin_slider_drag();
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn overview_begin_drag(&mut self) {
        self.overview.begin_drag();
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn overview_end_drag(&mut self) -> bool {
        self.overview.end_drag()
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn overview_drag_by(&mut self, dx: f64, dy: f64, horizontal: f32) {
        self.overview.drag_by(dx, dy, horizontal);
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn register_transition(
        &mut self,
        id: minimium_world::world::TransitionId,
        pending: PendingSectorTransition,
    ) -> bool {
        self.transitions.register(id, pending)
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn has_transition_id(&self, id: minimium_world::world::TransitionId) -> bool {
        self.transitions.contains_id(id)
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn commit_transition(
        &mut self,
        id: minimium_world::world::TransitionId,
        context: minimium_protocol::WorldContext,
    ) -> bool {
        self.transitions.commit(id, context)
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn has_transition(
        &self,
        id: minimium_world::world::TransitionId,
        context: minimium_protocol::WorldContext,
    ) -> bool {
        self.transitions.contains(id, context)
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn cancel_transition(
        &mut self,
        id: minimium_world::world::TransitionId,
        source: minimium_protocol::WorldContext,
    ) -> Option<PendingSectorTransition> {
        self.transitions.cancel(id, source)
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn fail_transition(
        &mut self,
        id: minimium_world::world::TransitionId,
    ) -> Option<PendingSectorTransition> {
        self.transitions.fail(id)
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn clear_transitions(&mut self) -> Vec<PendingSectorTransition> {
        self.transitions.take_pending()
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn is_loading(&self) -> bool {
        self.transitions.is_loading()
    }
    #[cfg(feature = "desktop")]
    pub(crate) fn set_loading(&mut self, loading: bool) {
        self.transitions.set_loading(loading);
    }
    #[cfg(feature = "desktop")]
    pub(crate) fn arm_map_teleport(&mut self) -> bool {
        self.transitions.arm_map_teleport()
    }
    #[cfg(feature = "desktop")]
    pub(crate) fn map_teleport_armed(&self) -> bool {
        self.transitions.map_teleport_armed()
    }
    #[cfg(feature = "desktop")]
    pub(crate) fn map_teleport_pending(&self) -> bool {
        self.transitions.map_teleport_pending()
    }
    #[cfg(feature = "desktop")]
    pub(crate) fn request_map_teleport(&mut self) -> bool {
        self.transitions.request_map_teleport()
    }
    #[cfg(feature = "desktop")]
    pub(crate) fn resolve_map_teleport(&mut self) {
        self.transitions.resolve_map_teleport();
    }

    pub(crate) fn start_remote(
        &mut self,
        address: String,
    ) -> Result<(), minimium_client_runtime::connector::StartError> {
        self.connection.start_remote(address)
    }

    pub(crate) fn start_local(
        &mut self,
        config: minimium_client_runtime::LocalServerLaunchConfig,
    ) -> Result<(), minimium_client_runtime::connector::StartError> {
        self.connection.start_local(config)
    }

    pub(crate) fn cancel_connection_attempt(&mut self) {
        self.connection.cancel();
    }

    pub(crate) fn poll_connector(&mut self) -> minimium_client_runtime::ConnectionEvent {
        self.connection.poll_connector()
    }

    pub(crate) fn poll_admin_result(
        &mut self,
    ) -> Option<Result<String, minimium_client_runtime::ControllerError>> {
        self.connection.poll_admin_result()
    }

    pub(crate) fn poll_session(&mut self, events: &mut Vec<crate::session::SessionEvent>) {
        self.connection.poll_session(events);
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn take_material_package_if_awaiting_gpu(
        &mut self,
    ) -> Option<Option<minimium_asset_io::materials::MaterialPackage>> {
        let session = self.connection.session_mut()?;
        session
            .content_awaiting_gpu()
            .then(|| session.take_material_package())
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn finish_material_content(&mut self) -> Option<Result<(), String>> {
        self.connection
            .session_mut()
            .map(crate::session::ClientSession::finish_content)
            .map(|result| result.map_err(|error| error.to_string()))
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
    ) -> Result<(), crate::session::QueueError> {
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
        self.connection.report_transfer_host_ready(ticket)
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
        if content.manifest != preparation.manifest
            || content.map.fingerprint() != preparation.map_fingerprint
        {
            return false;
        }
        let Some(terrain) = session.terrain_inputs_for_context(ticket.context()) else {
            return false;
        };
        if preparation.manifest.planet().is_some() && terrain.geography.is_none() {
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
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.connection.finish_until(deadline)
    }

    pub(crate) fn request_set_time(
        &mut self,
        minute: u16,
        longitude: f64,
    ) -> Result<(), minimium_client_runtime::ControllerError> {
        self.connection.request_set_time(minute, longitude)
    }

    pub(crate) fn request_time_scale(
        &mut self,
        value: String,
    ) -> Result<(), minimium_client_runtime::ControllerError> {
        self.connection.request_time_scale(value)
    }

    pub(crate) fn request_time_mode(
        &mut self,
        mode: &str,
    ) -> Result<(), minimium_client_runtime::ControllerError> {
        self.connection.request_time_mode(mode)
    }

    pub(crate) fn request_time_offline_policy(
        &mut self,
        policy: &str,
    ) -> Result<(), minimium_client_runtime::ControllerError> {
        self.connection.request_time_offline_policy(policy)
    }

    pub(crate) fn request_random_bolid(
        &mut self,
        count: u16,
    ) -> Result<(), minimium_client_runtime::ControllerError> {
        self.connection.request_random_bolid(count)
    }

    pub(crate) fn request_random_meteorid(
        &mut self,
    ) -> Result<(), minimium_client_runtime::ControllerError> {
        self.connection.request_random_meteorid()
    }

    pub(crate) fn execute_text_command(
        &mut self,
        input: &str,
        source: crate::client_commands::CommandSource,
        fire_direction: [f32; 3],
        player_position: Option<[f32; 3]>,
    ) -> crate::client_commands::TextCommandResult {
        let mut context = crate::client_commands::CommandContext {
            connection: &mut self.connection,
            action_executor: &mut self.action_executor,
            fire_direction,
            player_position,
        };
        crate::client_commands::execute_text(input, source, &mut context)
    }

    pub(crate) fn disconnect_session(&mut self) -> Option<crate::session::ClientSession> {
        self.game.set_session_context(None);
        self.pending_transfer = None;
        self.game.gameplay.clear_remote_players();
        self.game.gameplay.health = minimium_simulation::PLAYER_MAX_HEALTH;
        self.game.gameplay.sectors_ready = false;
        self.action_executor.clear_input_cooldown();
        #[cfg(feature = "desktop")]
        self.transitions.reset();
        self.connection.take_session()
    }
}
mod actions;
pub(crate) use actions::QueuedActionKind;
pub(crate) use actions::{ClientAction, ClientActionExecutor, LocalActionOutcome};
#[cfg(feature = "desktop")]
pub(crate) use actions::{SuppressionReason, UseTrigger};
