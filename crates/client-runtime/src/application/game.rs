use minimium_protocol::WorldContext;
use minimium_world::world::{WorldInstanceId, WorldManifest};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug)]
pub struct ReplicaPlayerSample {
    pub position: [f32; 3],
    pub server_tick: u64,
    pub server_time_ms: u64,
    pub received_at: crate::ClientTime,
}

pub(crate) struct GameplayState {
    pub(super) replica: Option<ReplicaPlayerSample>,
    pub(crate) reconciliation: crate::SnapshotReconciler,
    pub(crate) remote_positions_mm: Vec<[i64; 3]>,
    pub(crate) prediction: crate::ClientPrediction,
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
        now: crate::ClientTime,
    ) -> Result<bool, crate::ClientTimeError> {
        if !self.reconciliation.apply_remote_snapshot(
            server_time_ms,
            full_snapshot,
            players,
            removed_players,
            now,
        )? {
            return Ok(false);
        }
        self.replica = Some(ReplicaPlayerSample {
            position: self_position,
            server_tick,
            server_time_ms,
            received_at: now,
        });
        self.prediction
            .observe_authority(server_time_ms, self_position_mm);
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
        Ok(true)
    }

    pub(crate) fn clear_remote_players(&mut self) {
        self.replica = None;
        self.reconciliation.clear();
        self.prediction.clear_authority();
        self.remote_positions_mm.clear();
    }
}

pub(crate) use crate::PredictedStep;

impl ClientGameState {
    pub(crate) fn for_initial_world(
        current_instance: WorldInstanceId,
        spawn: [f32; 3],
        world_manifests: HashMap<WorldInstanceId, WorldManifest>,
    ) -> Self {
        let gravity = world_manifests
            .get(&current_instance)
            .copied()
            .map(minimium_world::world::GravityPolicy::for_manifest)
            .map(minimium_world::world::GravityPolicy::character_gravity)
            .unwrap_or_default();
        let character = minimium_simulation::CharacterState::at_spawn_with_gravity(spawn, gravity);
        let gameplay = GameplayState {
            replica: None,
            reconciliation: crate::SnapshotReconciler::default(),
            remote_positions_mm: Vec::new(),
            prediction: crate::ClientPrediction::new(character, None),
            health: minimium_simulation::PLAYER_MAX_HEALTH,
            collision_index: minimium_simulation::CollisionIndex::default(),
            collision_query: minimium_simulation::CollisionQuery::default(),
            collision_revision: 0,
            sectors_ready: false,
        };
        Self::new(gameplay, current_instance, world_manifests)
    }

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
        if self.session_context != context {
            self.gameplay.replica = None;
        }
        self.session_context = context;
    }

    pub(crate) fn view(&self) -> crate::ClientView<'_> {
        crate::ClientView::new(
            self.current_instance,
            self.session_context,
            self.world_manifests.get(&self.current_instance).copied(),
            self.gameplay.prediction.character,
            self.gameplay.prediction.position_mm,
            self.gameplay.health,
            self.gameplay.reconciliation.player_count(),
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
