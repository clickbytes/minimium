#[cfg(feature = "desktop")]
mod game;
mod settings;
#[cfg(feature = "desktop")]
mod transitions;
#[cfg(feature = "desktop")]
mod ui;

#[cfg(feature = "desktop")]
pub(crate) use game::{ClientGameState, GameplayState};
#[cfg(feature = "desktop")]
pub(crate) use settings::ClientSettings;
pub(crate) use settings::SettingsStore;
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

#[cfg(feature = "desktop")]
use minimium_client_runtime::ConnectionController;
#[cfg(feature = "desktop")]
use minimium_world::world::{WorldInstanceId, WorldMap};
#[cfg(feature = "desktop")]
use std::collections::HashMap;

/// Application-owned client state; presentation resources remain in the renderer.
#[cfg(feature = "desktop")]
pub(crate) struct ClientApplication {
    #[cfg(feature = "desktop")]
    pub(crate) ui: UiModel,
    #[cfg(feature = "desktop")]
    pub(crate) game: ClientGameState,
    pub(crate) connection: ConnectionController,
    pub(crate) settings: ClientSettings,
    pub(crate) input: crate::input::InputMapper,
    pub(crate) world_catalog: HashMap<WorldInstanceId, WorldMap>,
    transitions: WorldTransitionController,
    overview: MapOverviewModel,
    pub(crate) status: ClientStatus,
}

#[cfg(feature = "desktop")]
impl ClientApplication {
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
            settings,
            input,
            world_catalog,
            transitions,
            overview,
            status,
        }
    }

    pub(crate) fn overview(&self) -> &MapOverviewModel {
        &self.overview
    }

    pub(crate) fn overview_open(&mut self, instance: WorldInstanceId) {
        self.overview.open(instance);
    }

    pub(crate) fn overview_close(&mut self) {
        self.overview.close();
    }

    pub(crate) fn overview_set_zoom(&mut self, zoom: f32) {
        self.overview.set_zoom(zoom);
    }

    pub(crate) fn overview_begin_slider_drag(&mut self) {
        self.overview.begin_slider_drag();
    }

    pub(crate) fn overview_begin_drag(&mut self) {
        self.overview.begin_drag();
    }

    pub(crate) fn overview_end_drag(&mut self) -> bool {
        self.overview.end_drag()
    }

    pub(crate) fn overview_drag_by(&mut self, dx: f64, dy: f64, horizontal: f32) {
        self.overview.drag_by(dx, dy, horizontal);
    }

    pub(crate) fn register_transition(
        &mut self,
        id: minimium_world::world::TransitionId,
        pending: PendingSectorTransition,
    ) -> bool {
        self.transitions.register(id, pending)
    }

    pub(crate) fn pending_transition_preparations(
        &self,
    ) -> Vec<(minimium_world::world::TransitionId, PendingSectorTransition)> {
        self.transitions.pending_preparations()
    }

    pub(crate) fn mark_transition_ready(
        &mut self,
        id: minimium_world::world::TransitionId,
    ) -> bool {
        self.transitions.mark_ready_sent(id)
    }

    pub(crate) fn commit_transition(
        &mut self,
        id: minimium_world::world::TransitionId,
        context: minimium_protocol::WorldContext,
    ) -> bool {
        self.transitions.commit(id, context)
    }

    pub(crate) fn can_commit_transition(
        &self,
        id: minimium_world::world::TransitionId,
        context: minimium_protocol::WorldContext,
    ) -> bool {
        self.transitions.can_commit(id, context)
    }

    pub(crate) fn cancel_transition(
        &mut self,
        id: minimium_world::world::TransitionId,
        source: minimium_protocol::WorldContext,
    ) -> Option<PendingSectorTransition> {
        self.transitions.cancel(id, source)
    }

    pub(crate) fn fail_transition(
        &mut self,
        id: minimium_world::world::TransitionId,
    ) -> Option<PendingSectorTransition> {
        self.transitions.fail(id)
    }

    pub(crate) fn clear_transitions(&mut self) -> Vec<PendingSectorTransition> {
        self.transitions.take_pending()
    }

    pub(crate) fn is_loading(&self) -> bool {
        self.transitions.is_loading()
    }
    pub(crate) fn set_loading(&mut self, loading: bool) {
        self.transitions.set_loading(loading);
    }
    pub(crate) fn arm_map_teleport(&mut self) -> bool {
        self.transitions.arm_map_teleport()
    }
    pub(crate) fn map_teleport_armed(&self) -> bool {
        self.transitions.map_teleport_armed()
    }
    pub(crate) fn map_teleport_pending(&self) -> bool {
        self.transitions.map_teleport_pending()
    }
    pub(crate) fn request_map_teleport(&mut self) -> bool {
        self.transitions.request_map_teleport()
    }
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

    #[cfg(feature = "desktop")]
    pub(crate) fn disconnect_session(&mut self) -> Option<crate::session::ClientSession> {
        self.game.set_session_context(None);
        self.transitions.reset();
        self.connection.take_session()
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn activate_world(&mut self, instance: WorldInstanceId) -> WorldInstanceId {
        let previous = self.game.current_instance;
        self.game.current_instance = instance;
        previous
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn install_world_content(
        &mut self,
        instance: WorldInstanceId,
        manifest: minimium_world::world::WorldManifest,
        map: WorldMap,
    ) {
        if self
            .game
            .world_manifests
            .get(&instance)
            .is_some_and(|current| *current != manifest)
        {
            self.transitions.remove_instance(instance);
        }
        self.game.world_manifests.insert(instance, manifest);
        self.world_catalog.insert(instance, map);
    }
}
