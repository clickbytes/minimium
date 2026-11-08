use minimium_protocol::WorldContext;
use minimium_world::world::{WorldInstanceId, WorldManifest};
use std::collections::HashMap;

pub(crate) struct GameplayState {
    pub(crate) reconciliation: minimium_client_runtime::SnapshotReconciler,
    pub(crate) remote_positions_mm: Vec<[i64; 3]>,
    pub(crate) prediction: minimium_client_runtime::ClientPrediction,
    pub(crate) health: u16,
    pub(crate) collision_index: minimium_simulation::CollisionIndex,
    pub(crate) collision_query: minimium_simulation::CollisionQuery,
    pub(crate) collision_revision: u64,
    pub(crate) sectors_ready: bool,
}

/// Logical world/session state owned by the application, without GPU or render-geometry handles.
pub(crate) struct ClientGameState {
    pub(crate) gameplay: GameplayState,
    pub(crate) current_instance: WorldInstanceId,
    pub(crate) world_manifests: HashMap<WorldInstanceId, WorldManifest>,
    session_context: Option<WorldContext>,
}

impl GameplayState {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn apply_snapshot(
        &mut self,
        server_time_ms: u64,
        full_snapshot: bool,
        server_tick: u64,
        resolved_input_seq: u64,
        self_position: [f32; 3],
        self_position_mm: minimium_world::world::Position3Mm,
        self_velocity: [f32; 3],
        self_grounded: bool,
        self_movement: minimium_simulation::MovementIntent,
        movement_policy: minimium_simulation::MovementPolicy,
        manifest: Option<WorldManifest>,
        geography: Option<std::sync::Arc<dyn minimium_procgen::PreparedGeography>>,
        players: &minimium_protocol::RemotePlayers,
        removed_players: &minimium_protocol::RemotePlayerIds,
    ) -> bool {
        if !self.reconciliation.apply_remote_snapshot(
            server_time_ms,
            full_snapshot,
            players,
            removed_players,
        ) {
            return false;
        }
        let precise_position = self
            .prediction
            .position_mm
            .is_some()
            .then_some(self_position_mm);
        self.prediction.reconcile_server(
            self_position,
            precise_position,
            self_velocity,
            self_grounded,
            self_movement,
            server_tick,
            resolved_input_seq,
            movement_policy.revision,
            self.collision_revision,
            &self.collision_index,
            &mut self.collision_query,
            manifest,
            geography,
        );
        true
    }

    pub(crate) fn clear_remote_players(&mut self) {
        self.reconciliation.clear();
        self.remote_positions_mm.clear();
    }
}

pub(crate) use minimium_client_runtime::PredictedStep;

impl ClientGameState {
    pub(crate) fn new(
        gameplay: GameplayState,
        current_instance: WorldInstanceId,
        world_manifests: HashMap<WorldInstanceId, WorldManifest>,
    ) -> Self {
        Self {
            gameplay,
            current_instance,
            world_manifests,
            session_context: None,
        }
    }

    pub(crate) fn session_context(&self) -> Option<WorldContext> {
        self.session_context
    }

    pub(crate) fn set_session_context(&mut self, context: Option<WorldContext>) {
        self.session_context = context;
    }

    pub(crate) fn view(&self) -> minimium_client_runtime::ClientView<'_> {
        minimium_client_runtime::ClientView::new(
            self.current_instance,
            self.world_manifests.get(&self.current_instance).copied(),
            self.gameplay.prediction.character,
            self.gameplay.prediction.position_mm,
            self.gameplay.health,
            &self.gameplay.remote_positions_mm,
            self.gameplay.sectors_ready,
        )
    }

    pub(crate) fn accumulate_prediction_time(&mut self, delta_seconds: f32) {
        self.gameplay.prediction.accumulate_time(delta_seconds);
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn predict_next(
        &mut self,
        direction: [f32; 2],
        vertical: f32,
        movement: minimium_simulation::MovementPolicy,
        jump_pending: bool,
        movement_enabled: bool,
        manifest: Option<WorldManifest>,
        geography: Option<std::sync::Arc<dyn minimium_procgen::PreparedGeography>>,
    ) -> Option<PredictedStep> {
        let gameplay = &mut self.gameplay;
        gameplay.prediction.predict_next(
            &gameplay.collision_index,
            &mut gameplay.collision_query,
            direction,
            vertical,
            movement,
            jump_pending,
            movement_enabled,
            manifest,
            geography,
        )
    }
}
