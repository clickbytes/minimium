use super::GameplayView;
use minimium_core::world::{WorldInstanceId, WorldManifest};
use minimium_protocol::WorldContext;
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

pub(super) struct PredictedStep {
    pub(super) input: minimium_core::world::CharacterInput,
    pub(super) previous_state: minimium_core::world::CharacterState,
    pub(super) previous_mm: Option<minimium_core::world::Position3Mm>,
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
    pub(super) fn accumulate_prediction_time(&mut self, delta_seconds: f32) {
        self.gameplay.physics_accumulator += delta_seconds;
    }

    /// Applies one fixed client prediction step, retaining rollback state for scene readiness.
    pub(super) fn predict_next(
        &mut self,
        direction: [f32; 2],
        jump_pending: bool,
        movement_enabled: bool,
        manifest: Option<WorldManifest>,
    ) -> Option<PredictedStep> {
        if self.gameplay.physics_accumulator < minimium_core::world::SIMULATION_STEP_SECONDS {
            return None;
        }
        let mut input = minimium_core::world::CharacterInput {
            direction: if movement_enabled {
                direction
            } else {
                [0.0; 2]
            },
            jump: jump_pending && movement_enabled,
        };
        let previous_state = self.gameplay.character;
        let previous_mm = self.gameplay.position_mm;
        self.predict(input);
        if manifest.is_some_and(|manifest| {
            let position_mm = self.gameplay.position_mm.unwrap_or_else(|| {
                minimium_core::world::Position3Mm(
                    self.gameplay
                        .character
                        .position
                        .map(|value| (f64::from(value) * 1000.0).round() as i64),
                )
            });
            !minimium_procgen::can_traverse_surface(manifest, position_mm)
        }) {
            self.gameplay.character = previous_state;
            self.gameplay.position_mm = previous_mm;
            self.gameplay.character.velocity = [0.0; 3];
            input = minimium_core::world::CharacterInput::default();
        }
        self.gameplay.physics_accumulator -= minimium_core::world::SIMULATION_STEP_SECONDS;
        Some(PredictedStep {
            input,
            previous_state,
            previous_mm,
        })
    }

    /// Advances one fixed simulation step using the active world's collision snapshot.
    pub(super) fn predict(&mut self, input: minimium_core::world::CharacterInput) {
        let Self {
            gameplay:
                super::GameplayView {
                    collision_index,
                    collision_query,
                    character,
                    position_mm,
                    ..
                },
            ..
        } = self;
        collision_query.fill(
            collision_index,
            *character,
            minimium_core::world::SIMULATION_STEP_SECONDS,
        );
        if let Some(position_mm) = position_mm {
            minimium_core::world::step_character_mm(
                character,
                position_mm,
                input,
                collision_query.candidates(),
                collision_query.surface_candidates(),
                minimium_core::world::SIMULATION_STEP_SECONDS,
            );
        } else {
            minimium_core::world::step_character(
                character,
                input,
                collision_query.candidates(),
                collision_query.surface_candidates(),
                minimium_core::world::SIMULATION_STEP_SECONDS,
            );
        }
    }
}
