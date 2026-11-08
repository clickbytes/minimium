//! Fixed-step local prediction around the shared simulation integrator.
use minimium_simulation::{
    CharacterState, CollisionIndex, CollisionQuery, MovementIntent, MovementPolicy,
};
use minimium_world::world::{Position3Mm, WorldManifest};
use std::collections::VecDeque;
use std::sync::Arc;

pub struct ClientPrediction {
    pub character: CharacterState,
    pub position_mm: Option<Position3Mm>,
    physics_accumulator: f32,
    pending_inputs: VecDeque<(u64, u64, MovementIntent, MovementPolicy, u64)>,
    resolved_intent: MovementIntent,
    visual_offset: [f32; 3],
}

pub struct PredictedStep {
    pub input: MovementIntent,
    pub previous_state: CharacterState,
    pub previous_mm: Option<Position3Mm>,
}

impl ClientPrediction {
    pub fn new(character: CharacterState, position_mm: Option<Position3Mm>) -> Self {
        Self {
            character,
            position_mm,
            physics_accumulator: 0.0,
            pending_inputs: VecDeque::with_capacity(128),
            resolved_intent: MovementIntent::default(),
            visual_offset: [0.0; 3],
        }
    }

    pub fn accumulate_time(&mut self, delta_seconds: f32) {
        self.physics_accumulator += delta_seconds;
    }

    pub fn reset_time(&mut self) {
        self.physics_accumulator = 0.0;
    }

    pub fn record_input(
        &mut self,
        sample: minimium_protocol::MovementSample,
        policy: MovementPolicy,
        collision_revision: u64,
    ) {
        if self.pending_inputs.len() == 128 {
            self.reset_history();
        }
        self.pending_inputs.push_back((
            sample.input_seq,
            sample.target_tick,
            sample.intent,
            policy,
            collision_revision,
        ));
    }

    pub fn reset(&mut self) {
        self.pending_inputs.clear();
        self.resolved_intent = MovementIntent::default();
        self.visual_offset = [0.0; 3];
        self.reset_time();
    }

    fn reset_history(&mut self) {
        self.pending_inputs.clear();
        self.visual_offset = [0.0; 3];
        self.reset_time();
    }

    pub fn discard_resolved_inputs(&mut self, resolved_input_seq: u64) {
        while self
            .pending_inputs
            .front()
            .is_some_and(|entry| entry.0 <= resolved_input_seq)
        {
            self.pending_inputs.pop_front();
        }
    }

    pub fn take_visual_offset(&mut self, delta_seconds: f32) -> [f32; 3] {
        let decay = (1.0 - delta_seconds.max(0.0) * 8.0).clamp(0.0, 1.0);
        self.visual_offset = self.visual_offset.map(|value| value * decay);
        self.visual_offset
    }

    pub fn reconcile_server(
        &mut self,
        server_position: [f32; 3],
        server_position_mm: Option<Position3Mm>,
        server_velocity: [f32; 3],
        grounded: bool,
        server_movement: MovementIntent,
        server_tick: u64,
        resolved_input_seq: u64,
        policy_revision: u64,
        collision_revision: u64,
        collision_index: &CollisionIndex,
        collision_query: &mut CollisionQuery,
        manifest: Option<WorldManifest>,
        geography: Option<Arc<dyn minimium_procgen::PreparedGeography>>,
    ) {
        let old = self.character.position;
        if let Some(server_position_mm) = server_position_mm {
            self.position_mm = Some(server_position_mm);
            self.character.position = server_position_mm.0.map(|value| value as f32 / 1000.0);
        } else {
            self.position_mm = None;
            self.character.position = server_position;
        }
        self.character.velocity = server_velocity;
        self.character.grounded = grounded;
        self.resolved_intent = server_movement;
        self.discard_resolved_inputs(resolved_input_seq);
        let replay: Vec<_> = self
            .pending_inputs
            .iter()
            .filter(|(_, tick, _, policy, revision)| {
                *tick > server_tick
                    && *revision == collision_revision
                    && policy.revision == policy_revision
            })
            .copied()
            .collect();
        if replay.len() != self.pending_inputs.len() {
            self.reset_history();
            return;
        }
        let Some((_, last_tick, _, _, _)) = replay.last().copied() else {
            self.visual_offset =
                std::array::from_fn(|axis| old[axis] - self.character.position[axis]);
            return;
        };
        let mut next_sample = 0;
        let mut held_input = self.resolved_intent;
        let mut active_policy = replay[0].3;
        for tick in server_tick.saturating_add(1)..=last_tick {
            let mut input = held_input;
            input.jump = false;
            while replay
                .get(next_sample)
                .is_some_and(|sample| sample.1 == tick)
            {
                let sample = replay[next_sample];
                held_input = sample.2;
                active_policy = sample.3;
                input = sample.2;
                next_sample += 1;
            }
            collision_query.fill_for_speed(
                collision_index,
                self.character,
                minimium_simulation::SIMULATION_STEP_SECONDS,
                active_policy.speed,
            );
            if let Some(position_mm) = &mut self.position_mm {
                minimium_simulation::step_character_mm(
                    &mut self.character,
                    position_mm,
                    input,
                    active_policy,
                    collision_query.candidates(),
                    collision_query.surface_candidates(),
                    minimium_simulation::SIMULATION_STEP_SECONDS,
                );
            } else {
                minimium_simulation::step_character(
                    &mut self.character,
                    input,
                    active_policy,
                    collision_query.candidates(),
                    collision_query.surface_candidates(),
                    minimium_simulation::SIMULATION_STEP_SECONDS,
                );
            }
            if !active_policy.can_fly
                && manifest.is_some_and(|m| {
                    let position = self.position_mm.unwrap_or_else(|| {
                        Position3Mm(
                            self.character
                                .position
                                .map(|v| (f64::from(v) * 1000.0).round() as i64),
                        )
                    });
                    !minimium_procgen::can_traverse_surface(m, position, geography.as_deref())
                })
            {
                self.reset_history();
                return;
            }
        }
        self.visual_offset = std::array::from_fn(|axis| old[axis] - self.character.position[axis]);
    }

    /// Applies one fixed step and restores a non-fly character that would leave land.
    #[allow(clippy::too_many_arguments)]
    pub fn predict_next(
        &mut self,
        collision_index: &CollisionIndex,
        collision_query: &mut CollisionQuery,
        direction: [f32; 2],
        vertical: f32,
        movement: MovementPolicy,
        jump_pending: bool,
        movement_enabled: bool,
        manifest: Option<WorldManifest>,
        geography: Option<Arc<dyn minimium_procgen::PreparedGeography>>,
    ) -> Option<PredictedStep> {
        if self.physics_accumulator < minimium_simulation::SIMULATION_STEP_SECONDS {
            return None;
        }
        let mut input = MovementIntent {
            direction: if movement_enabled {
                direction
            } else {
                [0.0; 2]
            },
            jump: jump_pending && movement_enabled,
            vertical: if movement_enabled { vertical } else { 0.0 },
        };
        let previous_state = self.character;
        let previous_mm = self.position_mm;
        collision_query.fill_for_speed(
            collision_index,
            self.character,
            minimium_simulation::SIMULATION_STEP_SECONDS,
            movement.speed,
        );
        if let Some(position_mm) = &mut self.position_mm {
            minimium_simulation::step_character_mm(
                &mut self.character,
                position_mm,
                input,
                movement,
                collision_query.candidates(),
                collision_query.surface_candidates(),
                minimium_simulation::SIMULATION_STEP_SECONDS,
            );
        } else {
            minimium_simulation::step_character(
                &mut self.character,
                input,
                movement,
                collision_query.candidates(),
                collision_query.surface_candidates(),
                minimium_simulation::SIMULATION_STEP_SECONDS,
            );
        }
        if !movement.can_fly
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
            input = MovementIntent::default();
        }
        self.physics_accumulator -= minimium_simulation::SIMULATION_STEP_SECONDS;
        Some(PredictedStep {
            input,
            previous_state,
            previous_mm,
        })
    }
}
