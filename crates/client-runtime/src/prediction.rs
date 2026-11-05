//! Fixed-step local prediction around the shared simulation integrator.
use minimium_simulation::{
    CharacterInput, CharacterState, CollisionIndex, CollisionQuery, MovementSettings,
};
use minimium_world::world::{Position3Mm, WorldManifest};
use std::sync::Arc;

pub struct ClientPrediction {
    pub character: CharacterState,
    pub position_mm: Option<Position3Mm>,
    physics_accumulator: f32,
}

pub struct PredictedStep {
    pub input: CharacterInput,
    pub previous_state: CharacterState,
    pub previous_mm: Option<Position3Mm>,
}

impl ClientPrediction {
    pub const fn new(character: CharacterState, position_mm: Option<Position3Mm>) -> Self {
        Self {
            character,
            position_mm,
            physics_accumulator: 0.0,
        }
    }

    pub fn accumulate_time(&mut self, delta_seconds: f32) {
        self.physics_accumulator += delta_seconds;
    }

    pub fn reset_time(&mut self) {
        self.physics_accumulator = 0.0;
    }

    pub fn reconcile_server(
        &mut self,
        server_position: [f32; 3],
        server_position_mm: Option<Position3Mm>,
        server_velocity: [f32; 3],
        grounded: bool,
    ) {
        if let Some(server_position_mm) = server_position_mm {
            if let Some(predicted_mm) = &mut self.position_mm {
                predicted_mm.0 = std::array::from_fn(|axis| {
                    (predicted_mm.0[axis] as f64 * 0.75 + server_position_mm.0[axis] as f64 * 0.25)
                        .round() as i64
                });
                self.character.position = predicted_mm.0.map(|value| value as f32 / 1000.0);
            } else {
                self.position_mm = Some(server_position_mm);
                self.character.position = server_position_mm.0.map(|value| value as f32 / 1000.0);
            }
        } else {
            self.character.position = std::array::from_fn(|axis| {
                self.character.position[axis] * 0.75 + server_position[axis] * 0.25
            });
            self.position_mm = None;
        }
        self.character.velocity = server_velocity;
        self.character.grounded = grounded;
    }

    /// Applies one fixed step and restores a non-fly character that would leave land.
    #[allow(clippy::too_many_arguments)]
    pub fn predict_next(
        &mut self,
        collision_index: &CollisionIndex,
        collision_query: &mut CollisionQuery,
        direction: [f32; 2],
        vertical: f32,
        movement: MovementSettings,
        jump_pending: bool,
        movement_enabled: bool,
        manifest: Option<WorldManifest>,
        geography: Option<Arc<dyn minimium_procgen::PreparedGeography>>,
    ) -> Option<PredictedStep> {
        if self.physics_accumulator < minimium_simulation::SIMULATION_STEP_SECONDS {
            return None;
        }
        let mut input = CharacterInput {
            direction: if movement_enabled {
                direction
            } else {
                [0.0; 2]
            },
            jump: jump_pending && movement_enabled,
            vertical: if movement_enabled { vertical } else { 0.0 },
            movement,
        };
        let previous_state = self.character;
        let previous_mm = self.position_mm;
        collision_query.fill_for_speed(
            collision_index,
            self.character,
            minimium_simulation::SIMULATION_STEP_SECONDS,
            input.movement.speed,
        );
        if let Some(position_mm) = &mut self.position_mm {
            minimium_simulation::step_character_mm(
                &mut self.character,
                position_mm,
                input,
                collision_query.candidates(),
                collision_query.surface_candidates(),
                minimium_simulation::SIMULATION_STEP_SECONDS,
            );
        } else {
            minimium_simulation::step_character(
                &mut self.character,
                input,
                collision_query.candidates(),
                collision_query.surface_candidates(),
                minimium_simulation::SIMULATION_STEP_SECONDS,
            );
        }
        if !movement.fly
            && manifest.is_some_and(|manifest| {
                let position_mm = self.position_mm.unwrap_or_else(|| {
                    Position3Mm(
                        self.character
                            .position
                            .map(|value| (f64::from(value) * 1000.0).round() as i64),
                    )
                });
                !minimium_procgen::can_traverse_surface(manifest, position_mm, geography.as_deref())
            })
        {
            self.character = previous_state;
            self.position_mm = previous_mm;
            self.character.velocity = [0.0; 3];
            input = CharacterInput::default();
        }
        self.physics_accumulator -= minimium_simulation::SIMULATION_STEP_SECONDS;
        Some(PredictedStep {
            input,
            previous_state,
            previous_mm,
        })
    }
}
