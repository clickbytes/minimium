use super::*;
use minimium_client_runtime::ClientTime;

impl ClientApplication {
    pub(crate) fn core_view(&self) -> minimium_client_runtime::ClientView<'_> {
        self.core.view()
    }

    pub(crate) fn validate_time(&self, now: ClientTime) -> Result<(), ClientTimeError> {
        self.core.validate_time(now)
    }

    pub(crate) fn world_catalog_view(&self) -> &HashMap<WorldInstanceId, WorldMap> {
        self.core.world_catalog()
    }

    pub(crate) fn world_manifest_catalog_view(
        &self,
    ) -> &HashMap<WorldInstanceId, minimium_world::world::WorldManifest> {
        self.core.world_manifest_catalog()
    }

    pub(crate) fn scene_terrain_inputs(
        &self,
    ) -> Result<Option<minimium_procgen::TerrainInputs>, minimium_procgen::GenerationError> {
        self.core.scene_terrain_inputs()
    }

    pub(crate) fn submerged_fraction(&self) -> f32 {
        self.core.submerged_fraction()
    }

    pub(crate) fn preview_tool_surface_hit(
        &self,
        origin: [f64; 3],
        direction: [f64; 3],
        reach_m: f64,
    ) -> Option<minimium_world::terrain::SurfaceHit> {
        self.core
            .preview_tool_surface_hit(origin, direction, reach_m)
    }

    pub(crate) fn execute_action(
        &mut self,
        action: ClientAction,
        now: ClientTime,
    ) -> LocalActionOutcome {
        self.core.execute_action(action, now)
    }

    pub(crate) fn report_active_scene_ready(
        &mut self,
        snapshot: minimium_client_runtime::world_readiness::CollisionSnapshot,
        now: ClientTime,
    ) -> bool {
        match self.core.report_active_scene_ready(snapshot, now) {
            Ok(ready) => ready,
            Err(error) => {
                self.core
                    .record_diagnostic_failure("client.invalid_time", &error);
                false
            }
        }
    }

    pub(crate) fn resolved_map_for_context(
        &self,
        context: minimium_protocol::WorldContext,
        fingerprint: u64,
        manifest: minimium_world::world::WorldManifest,
    ) -> Option<std::sync::Arc<WorldMap>> {
        self.core
            .resolved_map_for_context(context, fingerprint, manifest)
    }

    pub(crate) fn interpolate_remote_players(
        &mut self,
        now: ClientTime,
        delay: std::time::Duration,
    ) -> Result<bool, ClientTimeError> {
        self.core.interpolate_remote_players(now, delay)
    }

    pub(crate) fn invalidate_active_scene(
        &mut self,
        context: Option<minimium_protocol::WorldContext>,
        instance: WorldInstanceId,
        now: ClientTime,
    ) -> bool {
        match self.core.invalidate_active_scene(context, instance, now) {
            Ok(invalidated) => invalidated,
            Err(error) => {
                self.core
                    .record_diagnostic_failure("client.invalid_time", &error);
                false
            }
        }
    }

    pub(crate) fn report_sky_view(
        &mut self,
        context: minimium_protocol::WorldContext,
        direction: [f32; 3],
        night: bool,
        now: ClientTime,
    ) -> Result<bool, ClientTimeError> {
        self.core.report_sky_view(context, direction, night, now)
    }

    pub(crate) fn cancel_transition_request(
        &mut self,
        now: ClientTime,
    ) -> Result<(), ClientTimeError> {
        self.core.cancel_transition_request(now)
    }

    pub(crate) fn request_pending_transition_cancel(
        &mut self,
        now: ClientTime,
    ) -> Result<bool, ClientTimeError> {
        self.core.request_pending_transition_cancel(now)
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
    pub(crate) fn clear_map_teleport_arm(&mut self) {
        self.transitions.clear_map_teleport_arm();
    }
    pub(crate) fn map_teleport_armed(&self) -> bool {
        self.transitions.map_teleport_armed()
    }
    pub(crate) fn map_teleport_pending(&self) -> bool {
        self.core.map_teleport_pending()
    }

    pub(crate) fn take_material_package_if_awaiting_gpu(
        &mut self,
        now: ClientTime,
    ) -> Result<Option<Option<minimium_asset_io::materials::MaterialPackage>>, ClientTimeError>
    {
        self.core.take_material_package_if_awaiting_gpu(now)
    }
    pub(crate) fn finish_material_content(
        &mut self,
        now: ClientTime,
    ) -> Option<Result<(), String>> {
        match self.core.finish_material_content(now) {
            Ok(result) => result.map(Ok),
            Err(error) => Some(Err(error.to_string())),
        }
    }
}
