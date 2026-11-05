use super::GameplayView;
use minimium_protocol::WorldContext;
use minimium_world::world::{WorldInstanceId, WorldManifest};
use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
};

/// Client-owned simulation and logical world/session state, independent of wgpu.
pub(super) struct ClientGameState {
    pub(super) gameplay: GameplayView,
    pub(super) current_instance: WorldInstanceId,
    pub(super) world_manifests: HashMap<WorldInstanceId, WorldManifest>,
    pub(super) session_context: Option<WorldContext>,
}

pub(super) use minimium_client_runtime::PredictedStep;

impl Deref for GameplayView {
    type Target = minimium_client_runtime::ClientPrediction;

    fn deref(&self) -> &Self::Target {
        &self.prediction
    }
}

impl DerefMut for GameplayView {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.prediction
    }
}

impl Deref for ClientGameState {
    type Target = GameplayView;

    fn deref(&self) -> &Self::Target {
        &self.gameplay
    }
}

impl DerefMut for ClientGameState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.gameplay
    }
}

impl ClientGameState {
    pub(super) fn view(&self) -> minimium_client_runtime::ClientView<'_> {
        minimium_client_runtime::ClientView::new(
            self.current_instance,
            self.world_manifests.get(&self.current_instance).copied(),
            self.character,
            self.position_mm,
            self.health,
            &self.remote_positions_mm,
            self.sectors_ready,
        )
    }

    pub(super) fn accumulate_prediction_time(&mut self, delta_seconds: f32) {
        self.gameplay.prediction.accumulate_time(delta_seconds);
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn predict_next(
        &mut self,
        direction: [f32; 2],
        vertical: f32,
        movement: minimium_simulation::MovementSettings,
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

impl GameplayView {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply_snapshot(
        &mut self,
        server_time_ms: u64,
        full_snapshot: bool,
        self_position: [f32; 3],
        self_position_mm: minimium_world::world::Position3Mm,
        self_velocity: [f32; 3],
        self_grounded: bool,
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
        );
        self.remote_geometry_dirty = true;
        true
    }
}
