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
    authority: [Option<(u64, Position3Mm)>; 2],
    physics_accumulator: f32,
    pending_inputs: VecDeque<(u64, u64, MovementIntent, MovementPolicy, u64)>,
    resolved_intent: MovementIntent,
    visual_offset: [f32; 3],
    water_read: Option<Arc<dyn minimium_world::fluid::WaterRead>>,
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
            authority: [None; 2],
            physics_accumulator: 0.0,
            pending_inputs: VecDeque::with_capacity(128),
            resolved_intent: MovementIntent::default(),
            visual_offset: [0.0; 3],
            water_read: None,
        }
    }

    pub fn accumulate_time(&mut self, delta_seconds: f32) {
        self.physics_accumulator += delta_seconds;
    }

    /// Consumes one due input interval while collision data is unavailable,
    /// without integrating against an unproved index. The host can queue its
    /// existing neutral intent at fixed-step cadence; no position/history or
    /// input sequence is changed here.
    pub fn skip_unready_step(&mut self) -> bool {
        if self.physics_accumulator < minimium_simulation::SIMULATION_STEP_SECONDS {
            return false;
        }
        self.physics_accumulator -= minimium_simulation::SIMULATION_STEP_SECONDS;
        true
    }

    pub fn set_water_read(&mut self, water: Option<Arc<dyn minimium_world::fluid::WaterRead>>) {
        self.water_read = water;
    }

    pub fn is_submerged(&self) -> bool {
        self.sample_water()
            .is_some_and(|s| s.submerged_fraction > 0.05)
    }
    pub fn submerged_fraction(&self) -> f32 {
        self.sample_water()
            .map_or(0.0, |sample| sample.submerged_fraction as f32)
    }

    fn feet_m(&self) -> [f64; 3] {
        self.position_mm.map_or_else(
            || self.character.position.map(f64::from),
            |position| position.0.map(|v| v as f64 / 1000.0),
        )
    }

    fn sample_water(&self) -> Option<minimium_world::fluid::WaterSample> {
        let reader = self.water_read.as_ref()?;
        let feet = self.feet_m();
        let up = match self.character.gravity {
            minimium_world::world::CharacterGravity::FixedY => [0.0, 1.0, 0.0],
            minimium_world::world::CharacterGravity::Radial { center } => {
                let delta: [f64; 3] =
                    std::array::from_fn(|axis| feet[axis] - f64::from(center[axis]));
                let length = delta.iter().map(|v| v * v).sum::<f64>().sqrt();
                if length > f64::EPSILON {
                    delta.map(|v| v / length)
                } else {
                    [0.0, 1.0, 0.0]
                }
            }
        };
        Some(reader.sample_capsule(feet, up, minimium_world::world::PLAYER_HEIGHT as f64))
    }

    pub(crate) fn clear_authority(&mut self) {
        self.authority = [None; 2];
    }

    pub(crate) fn observe_authority(&mut self, ms: u64, position: Position3Mm) {
        if self.authority[1].is_none_or(|(last, _)| last != ms) {
            self.authority[0] = self.authority[1];
        }
        self.authority[1] = Some((ms, position));
    }

    pub(crate) fn interpolate_authority(&mut self, ms: u64) {
        let Some((end, to)) = self.authority[1] else {
            return;
        };
        let (start, from) = self.authority[0].unwrap_or((end, to));
        let span = end.saturating_sub(start);
        let distant = (0..3).any(|axis| from.0[axis].abs_diff(to.0[axis]) > 100_000);
        let fraction = if span == 0 || distant {
            1.0
        } else {
            ms.saturating_sub(start).min(span) as f64 / span as f64
        };
        let position = Position3Mm(std::array::from_fn(|axis| {
            (from.0[axis] as f64 + (to.0[axis] as f64 - from.0[axis] as f64) * fraction).round()
                as i64
        }));
        self.character.position = position.0.map(|v| (v as f64 / 1000.0) as f32);
        if self.position_mm.is_some() {
            self.position_mm = Some(position);
        }
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
        let decay = (-8.0 * delta_seconds.max(0.0)).exp();
        self.visual_offset = self.visual_offset.map(|value| value * decay);
        self.visual_offset
    }

    pub(crate) fn visual_offset(&self) -> [f32; 3] {
        self.visual_offset
    }

    fn preserve_visual_position(&mut self, previous_feet: [f64; 3]) {
        let feet = self.feet_m();
        // Several snapshots can arrive before the next frame. Carry the remaining
        // correction across all of them, subtracting positions at world precision.
        for axis in 0..3 {
            self.visual_offset[axis] += (previous_feet[axis] - feet[axis]) as f32;
        }
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
        let previous_feet = self.feet_m();
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
            self.preserve_visual_position(previous_feet);
            return;
        };
        if last_tick.saturating_sub(server_tick) > 8 {
            self.reset_history();
            return;
        }
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
                let jump = input.jump || sample.2.jump;
                input = sample.2;
                input.jump = jump;
                next_sample += 1;
            }
            collision_query.fill_for_policy(
                collision_index,
                self.character,
                minimium_simulation::SIMULATION_STEP_SECONDS,
                active_policy,
            );
            let water_sample = self.sample_water();
            if let Some(position_mm) = &mut self.position_mm {
                minimium_simulation::step_character_mm_with_water(
                    &mut self.character,
                    position_mm,
                    input,
                    active_policy,
                    collision_query.candidates(),
                    collision_query.surface_candidates(),
                    minimium_simulation::SIMULATION_STEP_SECONDS,
                    water_sample,
                );
            } else {
                minimium_simulation::step_character_with_water(
                    &mut self.character,
                    input,
                    active_policy,
                    collision_query.candidates(),
                    collision_query.surface_candidates(),
                    minimium_simulation::SIMULATION_STEP_SECONDS,
                    water_sample,
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
        self.preserve_visual_position(previous_feet);
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
        collision_query.fill_for_policy(
            collision_index,
            self.character,
            minimium_simulation::SIMULATION_STEP_SECONDS,
            movement,
        );
        let water_sample = self.sample_water();
        if let Some(position_mm) = &mut self.position_mm {
            minimium_simulation::step_character_mm_with_water(
                &mut self.character,
                position_mm,
                input,
                movement,
                collision_query.candidates(),
                collision_query.surface_candidates(),
                minimium_simulation::SIMULATION_STEP_SECONDS,
                water_sample,
            );
        } else {
            minimium_simulation::step_character_with_water(
                &mut self.character,
                input,
                movement,
                collision_query.candidates(),
                collision_query.surface_candidates(),
                minimium_simulation::SIMULATION_STEP_SECONDS,
                water_sample,
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
