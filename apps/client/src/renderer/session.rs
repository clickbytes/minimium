mod message_handling;
mod sector_readiness;

use super::*;
use crate::messages::ClientText as Text;

impl Renderer {
    pub(crate) fn start_remote_attempt(&mut self, address: &str) {
        self.demo_environment = None;
        self.environment_frame = crate::environment::EnvironmentFrameInput::unavailable();
        if self.application.settings.require_earth {
            self.application.status = crate::application::ClientStatus::Status(
                minimium_localization::UiStatus::EarthConnecting,
            );
            self.gpu.scene_visible = false;
        }
        match self.application.start_remote(address.to_owned()) {
            Ok(()) => self.application.ui.console.push_client_text(
                Text::ConnectingTo(address.to_owned()),
                false,
                &self.localizer,
                self.application.settings.locale,
            ),
            Err(error) => self.report_start_error(error),
        }
    }

    pub(crate) fn start_local_attempt(&mut self, threads: Option<crate::options::ThreadLane>) {
        self.demo_environment = None;
        self.environment_frame = crate::environment::EnvironmentFrameInput::unavailable();
        if self.application.settings.require_earth {
            self.application.status = crate::application::ClientStatus::Status(
                minimium_localization::UiStatus::EarthConnecting,
            );
            self.gpu.scene_visible = false;
        }
        let mut config = self.application.settings.local_server_config.clone();
        if let Some(threads) = threads {
            if config
                .settings
                .insert("server.threads".to_owned(), threads.as_str().to_owned())
                .is_some()
            {
                self.application.ui.console.push_ui_error_message(
                    minimium_localization::UiMessage::Notice(
                        minimium_localization::UiNotice::LocalServerThreadsConflict,
                    ),
                    &self.localizer,
                    self.application.settings.locale,
                );
                return;
            }
        }
        match self.application.start_local(config) {
            Ok(()) => self.application.ui.console.push_client_text(
                Text::StartingLocalServer,
                false,
                &self.localizer,
                self.application.settings.locale,
            ),
            Err(error) => self.report_start_error(error),
        }
    }

    fn report_start_error(&mut self, error: StartError) {
        let text = match error {
            StartError::Busy { active_attempt } => Text::BusyAttempt {
                attempt_id: active_attempt.get(),
            },
            StartError::AttemptIdsExhausted => Text::AttemptIdsExhausted,
            StartError::WorkerSpawn(error) => Text::WorkerStartFailed(error.to_string()),
        };
        if self.application.settings.require_earth {
            self.application.status = crate::application::ClientStatus::ClientText(text.clone());
        }
        self.application.ui.console.push_client_text(
            text,
            true,
            &self.localizer,
            self.application.settings.locale,
        );
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
        self.gpu.remote_vertices.clear();
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
        self.pending_bolid_request = false;
        self.last_sky_view_at = None;
        self.bolids.clear();
        self.diagnostic_bolid_age = None;
        self.clear_pending_sector_transitions();
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
        self.gpu.remote_vertices.clear();
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
        if !self
            .application
            .connection
            .session()
            .is_some_and(|session| session.state() == SessionState::Connected)
        {
            return;
        }
        let now = Instant::now();
        if self
            .gameplay_presentation
            .last_shot_sent_at
            .is_some_and(|last_shot| now.saturating_duration_since(last_shot) < SHOT_LOCAL_COOLDOWN)
        {
            return;
        }
        let direction = super::camera::camera_direction(
            self.application.game.gameplay.prediction.character,
            self.input.yaw,
            self.input.pitch,
        )
        .normalize()
        .to_array();
        if let Some(session) = self.application.connection.session_mut() {
            match session.queue_world_input(minimium_protocol::ClientWorldInput::Fire { direction })
            {
                Ok(_) => {
                    self.gameplay_presentation.last_shot_sent_at = Some(now);
                    if self.application.ui.console.open {
                        self.application.ui.console.push_client_text(
                            Text::ShotFired,
                            false,
                            &self.localizer,
                            self.application.settings.locale,
                        );
                    }
                }
                Err(error) if self.application.ui.console.open => {
                    self.application.ui.console.push_client_text(
                        Text::CouldNotFire(error.into()),
                        true,
                        &self.localizer,
                        self.application.settings.locale,
                    )
                }
                Err(_) => {}
            }
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
        if let Some(session) = self.application.connection.session_mut() {
            if session.state() == SessionState::Connected {
                let _ = session.queue_world_input(
                    minimium_protocol::ClientWorldInput::InteractEntity { entity },
                );
            }
        }
    }

    pub(super) fn update_remote_player_rendering(&mut self, now: Instant) {
        if self.application.connection.session().is_none() {
            self.application.game.gameplay.clear_remote_players();
            self.gameplay_presentation.clear_remote_players();
            self.gpu.remote_vertices.clear();
            self.gpu.update_remote_shadow_vertices(&[], None);
            return;
        }
        if self.gameplay_presentation.update_remote_vertices(
            &mut self.application.game.gameplay,
            now,
            self.gpu.render_origin,
        ) {
            self.gpu
                .update_remote_vertices(&self.gameplay_presentation.remote_vertices);
        }
        let center = self
            .application
            .game
            .gameplay
            .prediction
            .position_mm
            .map_or_else(
                || {
                    self.application
                        .game
                        .gameplay
                        .prediction
                        .character
                        .position
                        .map(f64::from)
                },
                |position| position.0.map(|value| value as f64 / 1000.0),
            );
        let shadow_vertices_changed = self.gameplay_presentation.update_remote_shadow_vertices(
            &self.application.game.gameplay,
            center,
            self.gpu.render_origin,
        );
        let geometry_stamp = self
            .gpu
            .world_geometry
            .publication()
            .map(|publication| publication.stamp);
        // Even an unchanged (or empty) player batch must acknowledge a new
        // terrain publication. Otherwise the shadow frame stays incomplete
        // until a remote player happens to move.
        if self.gpu.shadow_remote_dirty
            || shadow_vertices_changed
            || self.gpu.shadow_remote_stamp != geometry_stamp
        {
            self.gpu.update_remote_shadow_vertices(
                &self.gameplay_presentation.remote_shadow_vertices,
                geometry_stamp,
            );
        }
    }
}

mod world_install;
