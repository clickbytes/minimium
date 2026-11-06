use super::{cube, GameplayView, Vertex, REMOTE_INTERPOLATION_DELAY};
use minimium_protocol::WorldContext;
use minimium_world::world::{WorldInstanceId, WorldManifest};
use std::{collections::HashMap, time::Instant};

/// Client-owned simulation and logical world/session state, independent of wgpu.
pub(super) struct ClientGameState {
    pub(super) gameplay: GameplayView,
    pub(super) current_instance: WorldInstanceId,
    pub(super) world_manifests: HashMap<WorldInstanceId, WorldManifest>,
    pub(super) session_context: Option<WorldContext>,
}

pub(super) use minimium_client_runtime::PredictedStep;

impl ClientGameState {
    pub(super) fn view(&self) -> minimium_client_runtime::ClientView<'_> {
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

    pub(super) fn clear_remote_players(&mut self) {
        self.reconciliation.clear();
        self.last_shot_sent_at = None;
        self.remote_vertices.clear();
        self.remote_shadow_vertices.clear();
        self.remote_geometry_dirty = false;
    }

    pub(super) fn update_remote_vertices(&mut self, now: Instant, origin: [f32; 3]) -> bool {
        let moved = self.reconciliation.interpolate_positions(
            now,
            REMOTE_INTERPOLATION_DELAY,
            &mut self.remote_positions_mm,
        );
        if !moved && !self.remote_geometry_dirty {
            return false;
        }
        self.remote_vertices.clear();
        for position_mm in &self.remote_positions_mm {
            let position: [f32; 3] = std::array::from_fn(|axis| {
                (position_mm[axis] as f64 / 1000.0 - f64::from(origin[axis])) as f32
            });
            cube(
                &mut self.remote_vertices,
                [position[0], position[1] + 0.85, position[2]],
                [0.6, 1.7, 0.6],
                [0.82, 0.16, 0.12],
            );
        }
        self.remote_geometry_dirty = false;
        true
    }

    pub(super) fn update_remote_shadow_vertices(
        &mut self,
        center: [f64; 3],
        origin: [f32; 3],
    ) -> bool {
        let mut shadow_vertices = Vec::with_capacity(self.reconciliation.player_count() * 36);
        for track in self.reconciliation.tracks().iter().flatten() {
            let feet = track
                .displayed_position_mm
                .map(|value| value as f64 / 1000.0);
            let shape_center = [feet[0], feet[1] + 0.85, feet[2]];
            let half = [0.3, 0.85, 0.3];
            let distance_squared = (0..3)
                .map(|axis| {
                    let delta = (center[axis] - shape_center[axis]).abs() - half[axis];
                    delta.max(0.0).powi(2)
                })
                .sum::<f64>();
            if distance_squared > 256.0_f64.powi(2) {
                continue;
            }
            let relative: [f32; 3] =
                std::array::from_fn(|axis| (feet[axis] - f64::from(origin[axis])) as f32);
            cube(
                &mut shadow_vertices,
                [relative[0], relative[1] + 0.85, relative[2]],
                [0.6, 1.7, 0.6],
                [0.82, 0.16, 0.12],
            );
        }
        let changed = bytemuck::cast_slice::<Vertex, u8>(&shadow_vertices)
            != bytemuck::cast_slice::<Vertex, u8>(&self.remote_shadow_vertices);
        if changed {
            self.remote_shadow_vertices = shadow_vertices;
        }
        changed
    }
}
