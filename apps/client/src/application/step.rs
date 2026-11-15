use super::messages::WorldContextChange;
use super::{ApplicationMessageError, ClientApplication};
use crate::session::{SessionEvent, SessionState};
use minimium_client_runtime::ConnectionEvent;
use minimium_protocol::ServerMessage;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PredictionInput {
    pub(crate) direction: [f32; 2],
    pub(crate) vertical: f32,
    pub(crate) jump_pending: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ClientStepInput {
    pub(crate) now: Instant,
    /// `None` is the CPU-only host: it polls and applies session state without
    /// synthesizing desktop movement or Motion messages.
    pub(crate) prediction: Option<PredictionInput>,
}

pub(crate) struct AppliedSessionEvent {
    pub(crate) event: SessionEvent,
    pub(crate) update: Result<Option<WorldContextChange>, ApplicationMessageError>,
}

pub(crate) struct ClientStepOutput {
    pub(crate) admin_result: Option<Result<String, minimium_client_runtime::ControllerError>>,
    pub(crate) connection: ConnectionEvent,
    pub(crate) session_events: Vec<AppliedSessionEvent>,
    pub(crate) disconnect: Option<ClientStepDisconnect>,
    #[cfg(feature = "desktop")]
    pub(crate) movement_queue_error: Option<crate::session::QueueError>,
    #[cfg(feature = "desktop")]
    pub(crate) jump_consumed: bool,
    #[cfg(feature = "desktop")]
    pub(crate) visual_offset: [f32; 3],
}

pub(crate) struct ClientStepDisconnect {
    pub(crate) previous_context: Option<minimium_protocol::WorldContext>,
    pub(crate) previous_instance: minimium_world::world::WorldInstanceId,
    pub(crate) session: Option<crate::session::ClientSession>,
    #[cfg(feature = "desktop")]
    pub(crate) pending_transitions: Vec<super::PendingSectorTransition>,
}

impl ClientApplication {
    /// Advances application-owned time, prediction and session lifecycles once.
    /// Hosts may prepare presentation/readiness before this call and consume its
    /// typed events afterwards; redraws never poll or step the session.
    pub(crate) fn step(&mut self, input: ClientStepInput) -> ClientStepOutput {
        let elapsed = self.elapsed_since_step(input.now);
        let (movement_queue_error, jump_consumed) =
            input.prediction.map_or((None, false), |prediction| {
                self.step_prediction(elapsed, prediction)
            });
        #[cfg(feature = "desktop")]
        let visual_offset = if input.prediction.is_some() {
            self.game
                .gameplay
                .prediction
                .take_visual_offset(elapsed.as_secs_f32())
        } else {
            [0.0; 3]
        };
        #[cfg(not(feature = "desktop"))]
        let _ = jump_consumed;

        let admin_result = self.poll_admin_result();
        let connection = self.poll_connector();
        let mut polled_events = Vec::with_capacity(16);
        if movement_queue_error.is_none() {
            self.poll_session(&mut polled_events);
        }
        let mut session_events = Vec::with_capacity(polled_events.len());
        let mut disconnect_requested = false;
        for event in polled_events {
            let update = match &event {
                SessionEvent::Message(message) => self.apply_step_message(message),
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
        let disconnect = (disconnect_requested
            || self
                .connection
                .session()
                .is_some_and(|session| session.state() == SessionState::Disconnected))
        .then(|| self.disconnect_for_step());

        ClientStepOutput {
            admin_result,
            connection,
            session_events,
            disconnect,
            #[cfg(feature = "desktop")]
            movement_queue_error,
            #[cfg(feature = "desktop")]
            jump_consumed,
            #[cfg(feature = "desktop")]
            visual_offset,
        }
    }

    fn disconnect_for_step(&mut self) -> ClientStepDisconnect {
        let previous_context = self.game.session_context();
        let previous_instance = self.game.current_instance;
        #[cfg(feature = "desktop")]
        let pending_transitions = self.clear_transitions();
        let session = self.disconnect_session();
        ClientStepDisconnect {
            previous_context,
            previous_instance,
            session,
            #[cfg(feature = "desktop")]
            pending_transitions,
        }
    }

    fn elapsed_since_step(&mut self, now: Instant) -> Duration {
        let elapsed = self.last_step_at.map_or(Duration::ZERO, |previous| {
            now.checked_duration_since(previous)
                .unwrap_or(Duration::ZERO)
        });
        self.last_step_at = Some(self.last_step_at.map_or(now, |previous| previous.max(now)));
        elapsed.min(Duration::from_secs_f32(
            minimium_simulation::SIMULATION_STEP_SECONDS,
        ))
    }

    fn step_prediction(
        &mut self,
        elapsed: Duration,
        input: PredictionInput,
    ) -> (Option<crate::session::QueueError>, bool) {
        let delta_seconds = elapsed.as_secs_f32();
        let water_read = self
            .connection
            .session()
            .and_then(|session| session.water_read());
        self.game.gameplay.prediction.set_water_read(water_read);
        self.game.accumulate_prediction_time(delta_seconds);

        let console_open = self.console_open();
        let overview_open = self.overview_open_for_step();
        let movement_policy = self.connection.movement;
        let can_fly = movement_policy.can_fly;
        let submerged = self.game.gameplay.prediction.is_submerged();
        let direction =
            if (self.game.gameplay.sectors_ready || can_fly || submerged) && !overview_open {
                input.direction
            } else {
                [0.0; 2]
            };
        let vertical =
            if console_open || overview_open || (!self.connection.flight_enabled && !submerged) {
                0.0
            } else {
                input.vertical
            };
        let movement_enabled = (self.game.gameplay.sectors_ready || can_fly) && !overview_open;
        let mut jump_pending = input.jump_pending;
        let mut jump_consumed = false;
        let mut queue_error = None;

        let manifest = self
            .game
            .world_manifests
            .get(&self.game.current_instance)
            .copied();
        let geography = self
            .connection
            .session()
            .and_then(|session| session.prepared_geography());
        while let Some(predicted) = self.game.predict_next(
            direction,
            vertical,
            movement_policy,
            jump_pending,
            movement_enabled,
            manifest,
            geography.clone(),
        ) {
            jump_pending = false;
            jump_consumed = input.jump_pending;
            let mut movement = predicted.input;
            if !self.game.gameplay.sectors_ready && !can_fly {
                self.game.gameplay.prediction.character = predicted.previous_state;
                self.game.gameplay.prediction.position_mm = predicted.previous_mm;
                movement = minimium_simulation::MovementIntent::default();
            }
            if self
                .connection
                .session()
                .is_some_and(|session| session.state() == SessionState::Connected)
            {
                match self
                    .action_executor
                    .execute(&mut self.connection, super::ClientAction::Motion(movement))
                {
                    super::LocalActionOutcome::Rejected { error, .. } => {
                        queue_error = Some(error);
                        break;
                    }
                    super::LocalActionOutcome::Queued {
                        movement: Some(sample),
                        ..
                    } => {
                        let collision_revision = self.game.gameplay.collision_revision;
                        self.game.gameplay.prediction.record_input(
                            sample,
                            movement_policy,
                            collision_revision,
                        );
                    }
                    _ => {}
                }
            }
        }
        (queue_error, jump_consumed)
    }

    fn apply_step_message(
        &mut self,
        message: &ServerMessage,
    ) -> Result<Option<WorldContextChange>, ApplicationMessageError> {
        match message {
            ServerMessage::Welcome { manifest, .. } => {
                #[cfg(not(feature = "desktop"))]
                let _ = manifest;
                #[cfg(feature = "desktop")]
                if self.settings.require_earth
                    && manifest.planet().is_none_or(|planet| planet.mode != 1)
                {
                    return Err(ApplicationMessageError::RequiredEarthUnavailable);
                }
                self.apply_world_message(message).map(Some)
            }
            ServerMessage::TransitionCommitted { .. } => {
                self.apply_world_message(message).map(Some)
            }
            _ => self.apply_game_message(message).map(|()| None),
        }
    }

    fn console_open(&self) -> bool {
        #[cfg(feature = "desktop")]
        {
            self.ui.console.open
        }
        #[cfg(not(feature = "desktop"))]
        {
            false
        }
    }

    fn overview_open_for_step(&self) -> bool {
        #[cfg(feature = "desktop")]
        {
            self.overview.is_open()
        }
        #[cfg(not(feature = "desktop"))]
        {
            false
        }
    }
}
