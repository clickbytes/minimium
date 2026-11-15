mod message_handling;
mod sector_readiness;

use super::*;
use crate::messages::ClientText as Text;

impl Renderer {
    pub(crate) fn start_remote_attempt(
        &mut self,
        address: &str,
    ) -> crate::client_commands::CommandActionResult {
        match self.application.start_remote(address.to_owned()) {
            Ok(()) => {
                self.connection_attempt_started();
                crate::client_commands::CommandActionResult {
                    outputs: vec![crate::client_commands::CommandOutput::ClientText {
                        message: Text::ConnectingTo(address.to_owned()),
                        error: false,
                    }],
                }
            }
            Err(error) => self.report_start_error(error),
        }
    }

    pub(crate) fn start_local_attempt(
        &mut self,
        threads: Option<crate::options::ThreadLane>,
    ) -> crate::client_commands::CommandActionResult {
        let mut config = self.application.settings.local_server_config.clone();
        if let Some(threads) = threads {
            if config
                .settings
                .insert("server.threads".to_owned(), threads.as_str().to_owned())
                .is_some()
            {
                return crate::client_commands::CommandActionResult {
                    outputs: vec![crate::client_commands::CommandOutput::UiError(
                        minimium_localization::UiMessage::Notice(
                            minimium_localization::UiNotice::LocalServerThreadsConflict,
                        ),
                    )],
                };
            }
        }
        match self.application.start_local(config) {
            Ok(()) => {
                self.connection_attempt_started();
                crate::client_commands::CommandActionResult {
                    outputs: vec![crate::client_commands::CommandOutput::ClientText {
                        message: Text::StartingLocalServer,
                        error: false,
                    }],
                }
            }
            Err(error) => self.report_start_error(error),
        }
    }

    fn connection_attempt_started(&mut self) {
        self.demo_environment = None;
        self.environment_frame = crate::environment::EnvironmentFrameInput::unavailable();
        if self.application.settings.require_earth {
            self.application.status = crate::application::ClientStatus::Status(
                minimium_localization::UiStatus::EarthConnecting,
            );
            self.gpu.scene_visible = false;
        }
    }

    fn report_start_error(
        &mut self,
        error: StartError,
    ) -> crate::client_commands::CommandActionResult {
        let text = match error {
            StartError::Busy {
                active_attempt: Some(active_attempt),
            } => Text::BusyAttempt {
                attempt_id: active_attempt.get(),
            },
            StartError::Busy {
                active_attempt: None,
            } => Text::DisconnectOrWait,
            StartError::AttemptIdsExhausted => Text::AttemptIdsExhausted,
            StartError::WorkerSpawn(error) => Text::WorkerStartFailed(error.to_string()),
        };
        if self.application.settings.require_earth {
            self.application.status = crate::application::ClientStatus::ClientText(text.clone());
        }
        crate::client_commands::CommandActionResult {
            outputs: vec![crate::client_commands::CommandOutput::ClientText {
                message: text,
                error: true,
            }],
        }
    }

    pub(super) fn poll_connector(&mut self) {
        if let Some(result) = self.application.connection.poll_admin_result() {
            match result {
                Ok(response) => self.application.ui.console.push_line(response.trim()),
                Err(error) => {
                    self.application
                        .connection
                        .record_diagnostic_failure("client.admin_response_failed", &error);
                    self.application.ui.console.push_line(error.to_string());
                }
            }
        }
        match self.application.connection.poll_connector() {
            ConnectionEvent::TimedOut => {
                self.application.status = crate::application::ClientStatus::Status(
                    minimium_localization::UiStatus::ConnectionTimedOut,
                );
                self.application.ui.console.push_client_text(
                    Text::AttemptTimedOut,
                    true,
                    &self.localizer,
                    self.application.settings.locale,
                );
            }
            ConnectionEvent::SessionStarted => {
                self.clear_pending_sector_transitions();
                self.reset_remote_gameplay();
                self.application.ui.console.push_client_text(
                    Text::AwaitingWelcome,
                    false,
                    &self.localizer,
                    self.application.settings.locale,
                );
            }
            ConnectionEvent::SetupFailed(error) => {
                self.application.status = crate::application::ClientStatus::Message(
                    minimium_localization::UiMessage::EarthError(error.to_string()),
                );
                self.application.ui.console.push_client_text(
                    Text::ConnectionSetupFailed(error.to_string()),
                    true,
                    &self.localizer,
                    self.application.settings.locale,
                );
            }
            ConnectionEvent::Failed(error) => {
                self.application.status = crate::application::ClientStatus::Message(
                    minimium_localization::UiMessage::EarthError(error.to_string()),
                );
                self.application.ui.console.push_client_text(
                    Text::ConnectFailed(error.to_string()),
                    true,
                    &self.localizer,
                    self.application.settings.locale,
                );
            }
            _ => {}
        }
    }

    fn reset_remote_gameplay(&mut self) {
        self.application.action_executor.clear_input_cooldown();
        self.gpu.clear_remote_vertices();
        self.application.game.gameplay.clear_remote_players();
        self.application.game.gameplay.health = PLAYER_MAX_HEALTH;
        self.gameplay_presentation.clear_remote_players();
    }

    pub(crate) fn finish_on_exit(&mut self, deadline: Instant) -> Result<(), Box<dyn Error>> {
        self.clear_connection();
        let worker_deadline = deadline - Duration::from_secs(2);
        let server_result = self.application.connection.finish_until(worker_deadline);
        let mut unfinished = self.sector_cache.shutdown_until(worker_deadline);
        if !self.weather.shutdown_until(worker_deadline) {
            unfinished.push("client-cloud-mesh");
        }
        if !unfinished.is_empty() {
            let worker_error = std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                format!(
                    "client renderer workers exceeded shutdown deadline: {}",
                    unfinished.join(",")
                ),
            );
            return match server_result {
                Ok(()) => Err(worker_error.into()),
                Err(error) => Err(std::io::Error::other(format!(
                    "owned server shutdown failed ({error}); {worker_error}"
                ))
                .into()),
            };
        }
        server_result?;
        Ok(())
    }

    pub(super) fn clear_connection(&mut self) {
        self.application.action_executor.clear_input_cooldown();
        self.excavation_debug = Default::default();
        self.pending_bolid_request = None;
        self.last_sky_view_at = None;
        self.bolids.clear();
        self.diagnostic_bolid_age = None;
        self.clear_pending_sector_transitions();
        let bootstrap = super::sector_scene::SceneInterestOwner::Bootstrap(
            self.application.game.current_instance,
        );
        self.sector_cache.release_interest(bootstrap);
        self.gpu
            .release_scene_stage(bootstrap.instance(), bootstrap);
        if let Some(context) = self.application.game.session_context() {
            let active = super::sector_scene::SceneInterestOwner::Active(context);
            self.sector_cache.release_interest(active);
            self.gpu.release_scene_stage(context.instance(), active);
        }
        if self.application.settings.require_earth {
            self.gpu.scene_visible = false;
            if !self.application.status.is_error() {
                self.application.status = crate::application::ClientStatus::Status(
                    minimium_localization::UiStatus::EarthDisconnected,
                );
            }
        }
        if let Some(mut session) = self.application.disconnect_session() {
            let metrics = session.metrics();
            self.metrics.previous_bytes_sent = self
                .metrics
                .previous_bytes_sent
                .saturating_add(metrics.bytes_sent);
            self.metrics.previous_bytes_received = self
                .metrics
                .previous_bytes_received
                .saturating_add(metrics.bytes_received);
            session.close();
        }
        self.gpu.clear_remote_vertices();
        self.application.game.gameplay.clear_remote_players();
        self.application.game.gameplay.health = PLAYER_MAX_HEALTH;
        self.gameplay_presentation.clear_remote_players();
    }

    pub(super) fn fail_connection(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.application.status = crate::application::ClientStatus::Message(
            minimium_localization::UiMessage::EarthError(message.clone()),
        );
        if self.application.settings.require_earth {
            self.gpu.scene_visible = false;
        }
        self.application
            .connection
            .record_diagnostic_failure("client_runtime.session_content_failed", &message);
        self.application.ui.console.push_ui_error_message(
            minimium_localization::UiMessage::ContentUnavailable,
            &self.localizer,
            self.application.settings.locale,
        );
        self.clear_connection();
    }

    pub(super) fn poll_session(&mut self) {
        if self.application.connection.session().is_some() {
            self.application.ui.session_events.clear();
            self.application
                .connection
                .poll_session(&mut self.application.ui.session_events);
            self.prepare_session_materials();
        }
        let mut events = std::mem::take(&mut self.application.ui.session_events);
        for event in events.drain(..) {
            match event {
                SessionEvent::PingRoundTrip(duration) => {
                    if self.application.ui.console.open {
                        self.application.ui.console.push_client_text(
                            Text::Ping {
                                milliseconds: duration.as_millis(),
                            },
                            false,
                            &self.localizer,
                            self.application.settings.locale,
                        );
                    }
                }
                SessionEvent::Disconnected(reason) => {
                    if reason == DisconnectReason::ServerClosed {
                        if self.application.settings.require_earth {
                            self.application.status = crate::application::ClientStatus::Status(
                                minimium_localization::UiStatus::EarthServerClosed,
                            );
                        }
                        if self.application.ui.console.open {
                            self.application.ui.console.push_client_text(
                                Text::ServerClosed,
                                false,
                                &self.localizer,
                                self.application.settings.locale,
                            );
                        }
                        self.clear_connection();
                        continue;
                    }
                    let text = Text::from(reason);
                    if self.application.settings.require_earth {
                        self.application.status =
                            crate::application::ClientStatus::ClientText(text.clone());
                        self.gpu.scene_visible = false;
                    }
                    if self.application.ui.console.open {
                        self.application.ui.console.push_client_text(
                            text,
                            true,
                            &self.localizer,
                            self.application.settings.locale,
                        );
                    }
                    self.clear_connection();
                }
                SessionEvent::Message(message) => {
                    self.handle_server_message(message);
                    if self.application.connection.session().is_none() {
                        break;
                    }
                }
            }
        }
        if self
            .application
            .connection
            .session()
            .is_some_and(|session| session.state() == SessionState::Disconnected)
        {
            self.clear_connection();
        }
        self.application.ui.session_events = events;
    }

    pub(super) fn queue_fire(&mut self) {
        let prediction = &self.application.game.gameplay.prediction;
        let character = prediction.character;
        let feet_m = prediction.position_mm.map_or_else(
            || character.position.map(f64::from),
            |position| position.0.map(|value| value as f64 / 1000.0),
        );
        let Some(geometry) = CharacterGeometry::new(feet_m, character.gravity) else {
            return;
        };
        let direction = super::camera::camera_direction_from_geometry(
            geometry,
            self.input.yaw,
            self.input.pitch,
        )
        .normalize()
        .to_array();
        match self.application.action_executor.execute(
            &mut self.application.connection,
            crate::application::ClientAction::UseEquipped {
                direction,
                trigger: crate::application::UseTrigger::Pressed,
            },
        ) {
            crate::application::LocalActionOutcome::Queued {
                kind: crate::application::QueuedActionKind::WeaponFire,
                ..
            } => {
                if self.application.ui.console.open {
                    self.application.ui.console.push_client_text(
                        Text::ShotFired,
                        false,
                        &self.localizer,
                        self.application.settings.locale,
                    );
                }
            }
            crate::application::LocalActionOutcome::Rejected {
                kind: crate::application::QueuedActionKind::ToolUse,
                error,
            } => self.fail_connection(format!("Could not use tool: {error:?}")),
            crate::application::LocalActionOutcome::Rejected {
                kind: crate::application::QueuedActionKind::WeaponFire,
                error,
            } if self.application.ui.console.open => self.application.ui.console.push_client_text(
                Text::CouldNotFire(error.into()),
                true,
                &self.localizer,
                self.application.settings.locale,
            ),
            _ => {}
        }
    }

    pub(crate) fn queue_portal(&mut self) {
        let position = self.application.game.gameplay.prediction.character.position;
        let entity = self
            .application
            .connection
            .session()
            .and_then(|session| session.replicated_entities())
            .and_then(|replicas| crate::client_commands::nearest_portal_entity(replicas, position));
        let Some(entity) = entity else { return };
        let _ = self.application.action_executor.execute(
            &mut self.application.connection,
            crate::application::ClientAction::InteractEntity { entity },
        );
    }
}

mod remote_players;
mod world_install;
