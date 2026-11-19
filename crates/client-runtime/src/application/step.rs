use super::messages::WorldContextChange;
use super::readiness;
use super::{ApplicationMessageError, ClientApplicationCore};
use crate::{ClientTime, ClientTimeError, ConnectionEvent, SessionEvent, SessionState};
use minimium_protocol::ServerMessage;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default)]
pub struct PredictionInput {
    pub direction: [f32; 2],
    pub vertical: f32,
    pub jump_pending: bool,
    pub suppress_horizontal: bool,
    pub suppress_vertical: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct ClientStepInput {
    pub now: ClientTime,
    /// `None` is the CPU-only host: it polls and applies session state without
    /// synthesizing desktop movement or Motion messages.
    pub prediction: Option<PredictionInput>,
}

#[derive(Clone)]
pub struct PredictionSceneRequest<'a> {
    pub context: Option<minimium_protocol::WorldContext>,
    pub instance: minimium_world::world::WorldInstanceId,
    pub manifest: minimium_world::world::WorldManifest,
    pub previous_position: [f32; 3],
    pub position: [f32; 3],
    pub terrain_inputs: minimium_procgen::TerrainInputs,
    pub world_catalog: &'a std::collections::HashMap<
        minimium_world::world::WorldInstanceId,
        minimium_world::world::WorldMap,
    >,
}

#[derive(Clone)]
pub struct PredictionPresentationRequest {
    pub manifest: minimium_world::world::WorldManifest,
    pub position: [f32; 3],
    pub terrain_inputs: minimium_procgen::TerrainInputs,
}

pub struct AppliedSessionEvent {
    pub event: SessionEvent,
    pub update: Result<Option<WorldContextChange>, ApplicationMessageError>,
}

pub struct ClientStepOutput {
    pub admin_result: Option<minimium_commands::admin::AdminReplyV1>,
    pub connection: ConnectionEvent,
    pub session_events: Vec<AppliedSessionEvent>,
    pub disconnect: Option<ClientStepDisconnect>,
    pub candidate_scene_error: Option<String>,
    pub movement_queue_error: Option<crate::QueueError>,
    pub jump_consumed: bool,
    pub candidate_scene_pending: bool,
    pub visual_offset: [f32; 3],
}

pub struct ClientStepDisconnect {
    pub previous_context: Option<minimium_protocol::WorldContext>,
    pub previous_instance: minimium_world::world::WorldInstanceId,
    pub session: Option<crate::ClientSession>,
}

#[derive(Default)]
pub(super) struct MotionStream {
    scope: Option<(
        minimium_environment::time::SessionNonce,
        minimium_protocol::WorldContext,
    )>,
    sent: Option<(minimium_simulation::MovementIntent, ClientTime)>,
    previous_ms: Option<u64>,
    local_jump: bool,
}

impl ClientApplicationCore {
    /// Advances application-owned time, prediction and session lifecycles once.
    /// Hosts may prepare presentation/readiness before this call and consume its
    /// typed events afterwards; redraws never poll or step the session.
    pub fn step(
        &mut self,
        input: ClientStepInput,
        mut candidate_readiness: impl FnMut(
            PredictionSceneRequest<'_>,
        ) -> Result<
            Option<crate::world_readiness::CollisionSnapshot>,
            String,
        >,
        mut presentation_ready: Option<
            &mut dyn FnMut(
                &crate::world_readiness::CollisionSnapshot,
                PredictionPresentationRequest,
            ) -> Result<bool, String>,
        >,
    ) -> Result<ClientStepOutput, ClientTimeError> {
        self.accept_time(input.now)?;
        let elapsed = self.elapsed_since_step(input.now)?;
        let admin_result = self.poll_admin_result(input.now)?;
        let connection = self.poll_connector(input.now)?;
        if matches!(connection, ConnectionEvent::SessionStarted) {
            self.clear_action_input_cooldown();
            self.game.gameplay.clear_remote_players();
        }
        let mut polled_events = Vec::with_capacity(16);
        self.poll_session(&mut polled_events, input.now)?;
        let mut session_events = Vec::with_capacity(polled_events.len());
        let mut disconnect_requested = false;
        for event in polled_events {
            let update = match &event {
                SessionEvent::Message(message) => self.apply_step_message(message, input.now),
                _ => Ok(None),
            };
            let stop = matches!(event, SessionEvent::Disconnected(_))
                || matches!(&event, SessionEvent::Message(ServerMessage::Bye))
                || update.is_err_and(|error| {
                    !matches!(error, ApplicationMessageError::StaleWorldContext)
                });
            disconnect_requested |= stop;
            session_events.push(AppliedSessionEvent { event, update });
            if stop {
                break;
            }
        }
        let disconnect = if disconnect_requested
            || self
                .connection
                .session()
                .is_some_and(|session| session.state() == SessionState::Disconnected)
        {
            Some(self.disconnect_for_step(input.now)?)
        } else {
            None
        };

        let (movement_queue_error, jump_consumed, candidate_scene_pending, candidate_scene_error) =
            input
                .prediction
                .map_or((None, false, false, None::<String>), |prediction| {
                    self.step_prediction(
                        elapsed,
                        prediction,
                        &mut candidate_readiness,
                        &mut presentation_ready,
                        input.now,
                    )
                });
        Ok(ClientStepOutput {
            admin_result,
            connection,
            session_events,
            disconnect,
            candidate_scene_error,
            movement_queue_error,
            jump_consumed,
            candidate_scene_pending,
            // Session events may have reconciled or reset the position this step.
            visual_offset: if input.prediction.is_some() {
                self.game.gameplay.prediction.visual_offset()
            } else {
                [0.0; 3]
            },
        })
    }

    fn disconnect_for_step(
        &mut self,
        now: ClientTime,
    ) -> Result<ClientStepDisconnect, ClientTimeError> {
        let previous_context = self.game.session_context();
        let previous_instance = self.game.current_instance;
        let session = self.disconnect_session(now)?;
        Ok(ClientStepDisconnect {
            previous_context,
            previous_instance,
            session,
        })
    }

    fn elapsed_since_step(&mut self, now: ClientTime) -> Result<Duration, ClientTimeError> {
        let elapsed = self.last_step_at.map_or(Ok(Duration::ZERO), |previous| {
            now.checked_duration_since(previous)
        })?;
        self.last_step_at = Some(now);
        Ok(elapsed)
    }

    fn step_prediction(
        &mut self,
        elapsed: Duration,
        input: PredictionInput,
        candidate_readiness: &mut impl FnMut(
            PredictionSceneRequest<'_>,
        ) -> Result<
            Option<crate::world_readiness::CollisionSnapshot>,
            String,
        >,
        presentation_ready: &mut Option<
            &mut dyn FnMut(
                &crate::world_readiness::CollisionSnapshot,
                PredictionPresentationRequest,
            ) -> Result<bool, String>,
        >,
        now: ClientTime,
    ) -> (Option<crate::QueueError>, bool, bool, Option<String>) {
        let session = self.connection.session();
        let sample = session.and_then(|s| s.clock_sample());
        let scope = sample
            .map(|s| s.session_nonce())
            .zip(self.game.session_context());
        if scope != self.motion_stream.scope {
            self.motion_stream = MotionStream {
                scope,
                ..Default::default()
            };
            self.game.gameplay.prediction.reset();
        }
        let presentation_ms = session.and_then(|s| s.environment_frame().simulation_ms);
        let delta = match presentation_ms {
            Some(ms) => Duration::from_millis(
                ms.saturating_sub(self.motion_stream.previous_ms.replace(ms).unwrap_or(ms)),
            ),
            None if session.is_none() => elapsed.min(Duration::from_millis(50)),
            None => Duration::ZERO,
        };
        if sample.is_some_and(|s| s.rate_ppm().get() == 0) {
            self.motion_stream.sent = None;
            self.motion_stream.local_jump = false;
            self.game.gameplay.prediction.reset();
            return (None, input.jump_pending, false, None);
        }
        // A long gap is reconciled from snapshots instead of an unbounded replay.
        let delta_seconds = if delta > Duration::from_millis(400) {
            self.game.gameplay.prediction.reset();
            self.game
                .gameplay
                .prediction
                .interpolate_authority(presentation_ms.unwrap_or(0));
            0.0
        } else {
            delta.as_secs_f32()
        };
        let water_read = self
            .connection
            .session()
            .and_then(|session| session.water_read());
        self.game.gameplay.prediction.set_water_read(water_read);
        self.game
            .gameplay
            .prediction
            .take_visual_offset(delta_seconds);
        self.game.accumulate_prediction_time(delta_seconds);
        let movement_policy = self.connection.movement;
        let can_fly = movement_policy.can_fly;
        let submerged = self.game.gameplay.prediction.is_submerged();
        let direction = if (self.game.gameplay.sectors_ready || can_fly || submerged)
            && !input.suppress_horizontal
        {
            input.direction
        } else {
            [0.0; 2]
        };
        let vertical = if input.suppress_vertical || (!self.connection.flight_enabled && !submerged)
        {
            0.0
        } else {
            input.vertical
        };
        let movement_enabled =
            (self.game.gameplay.sectors_ready || can_fly) && !input.suppress_horizontal;
        let mut movement = minimium_simulation::MovementIntent {
            direction,
            vertical,
            jump: input.jump_pending && movement_enabled,
        };
        self.motion_stream.local_jump |= movement.jump;
        let mut candidate_scene_pending = false;
        let mut candidate_scene_error = None;

        let manifest = self
            .game
            .world_manifests
            .get(&self.game.current_instance)
            .copied();
        let current_position = self.game.gameplay.prediction.character.position;
        let current_ready = match readiness::scene_ready(
            self,
            current_position,
            current_position,
            can_fly,
            candidate_readiness,
            presentation_ready,
        ) {
            Ok(ready) => ready,
            Err(error) => {
                candidate_scene_error = Some(error);
                false
            }
        };
        if !current_ready {
            candidate_scene_pending = true;
            self.game.gameplay.prediction.reset_time();
            self.motion_stream.local_jump = false;
            let error = self.transmit_motion(minimium_simulation::MovementIntent::default(), now);
            return (
                error,
                input.jump_pending,
                candidate_scene_pending,
                candidate_scene_error,
            );
        }
        let geography = self
            .connection
            .session()
            .and_then(|session| session.prepared_geography());
        while let Some(predicted) = self.game.predict_next(
            direction,
            vertical,
            movement_policy,
            self.motion_stream.local_jump,
            movement_enabled,
            manifest,
            geography.clone(),
        ) {
            self.motion_stream.local_jump = false;
            if predicted.input.direction == [0.0; 2] {
                movement.direction = [0.0; 2];
            }
            let candidate_position = self.game.gameplay.prediction.character.position;
            let candidate_ready = match readiness::scene_ready(
                self,
                predicted.previous_state.position,
                candidate_position,
                can_fly,
                candidate_readiness,
                presentation_ready,
            ) {
                Ok(ready) => ready,
                Err(error) => {
                    candidate_scene_error = Some(error);
                    false
                }
            };
            if !candidate_ready {
                candidate_scene_pending = true;
                self.game.gameplay.prediction.character = predicted.previous_state;
                self.game.gameplay.prediction.position_mm = predicted.previous_mm;
                movement = minimium_simulation::MovementIntent::default();
            }
        }
        let queue_error = self.transmit_motion(movement, now);
        let jump_consumed = input.jump_pending && queue_error.is_none();
        (
            queue_error,
            jump_consumed,
            candidate_scene_pending,
            candidate_scene_error,
        )
    }

    fn transmit_motion(
        &mut self,
        intent: minimium_simulation::MovementIntent,
        now: ClientTime,
    ) -> Option<crate::QueueError> {
        if !self
            .connection
            .session()
            .is_some_and(|s| s.state() == SessionState::Connected)
        {
            return None;
        }
        let held = intent.direction != [0.0; 2] || intent.vertical != 0.0;
        if self.motion_stream.sent.is_some_and(|(old, at)| {
            old == intent
                && !intent.jump
                && (!held || now.elapsed_since(at) < Duration::from_millis(100))
        }) {
            return None;
        }
        match self.action_executor.execute(
            &mut self.connection,
            super::ClientAction::Motion(intent),
            now,
        ) {
            super::LocalActionOutcome::Rejected { error, .. } => Some(error),
            super::LocalActionOutcome::Queued {
                movement: Some(sample),
                ..
            } => {
                self.motion_stream.sent = Some((intent, now));
                self.game.gameplay.prediction.record_input(
                    sample,
                    self.connection.movement,
                    self.game.gameplay.collision_revision,
                );
                None
            }
            _ => None,
        }
    }

    fn apply_step_message(
        &mut self,
        message: &ServerMessage,
        now: ClientTime,
    ) -> Result<Option<WorldContextChange>, ApplicationMessageError> {
        match message {
            ServerMessage::Welcome { manifest, .. } => {
                if self.require_earth && manifest.planet().is_none_or(|planet| planet.mode != 1) {
                    return Err(ApplicationMessageError::RequiredEarthUnavailable);
                }
                self.apply_world_message(message).map(Some)
            }
            ServerMessage::TransitionCommitted { .. } => {
                self.apply_world_message(message).map(Some)
            }
            _ => self.apply_game_message(message, now).map(|()| None),
        }
    }
}
