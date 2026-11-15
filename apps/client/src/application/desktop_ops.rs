use super::*;

impl ClientApplication {
    pub(crate) fn game_view(&self) -> &ClientGameState {
        &self.game
    }

    pub(crate) fn world_catalog_view(&self) -> &HashMap<WorldInstanceId, WorldMap> {
        &self.world_catalog
    }

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

    pub(crate) fn clear_remote_players(&mut self) {
        self.game.gameplay.clear_remote_players();
    }

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

    pub(crate) fn invalidate_active_scene(
        &mut self,
        context: Option<minimium_protocol::WorldContext>,
        instance: WorldInstanceId,
    ) {
        if self.game.session_context() == context && self.game.current_instance == instance {
            self.game.gameplay.sectors_ready = false;
        }
    }

    pub(crate) fn clear_action_input_cooldown(&mut self) {
        self.action_executor.clear_input_cooldown();
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

    pub(crate) fn has_transition_id(&self, id: minimium_world::world::TransitionId) -> bool {
        self.transitions.contains_id(id)
    }

    pub(crate) fn commit_transition(
        &mut self,
        id: minimium_world::world::TransitionId,
        context: minimium_protocol::WorldContext,
    ) -> bool {
        self.transitions.commit(id, context)
    }

    pub(crate) fn has_transition(
        &self,
        id: minimium_world::world::TransitionId,
        context: minimium_protocol::WorldContext,
    ) -> bool {
        self.transitions.contains(id, context)
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

    pub(crate) fn take_material_package_if_awaiting_gpu(
        &mut self,
    ) -> Option<Option<minimium_asset_io::materials::MaterialPackage>> {
        let session = self.connection.session_mut()?;
        session
            .content_awaiting_gpu()
            .then(|| session.take_material_package())
    }

    pub(crate) fn finish_material_content(&mut self) -> Option<Result<(), String>> {
        self.connection
            .session_mut()
            .map(crate::session::ClientSession::finish_content)
            .map(|result| result.map_err(|error| error.to_string()))
    }
}
