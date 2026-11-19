mod message_handling;
mod sector_readiness;
pub(in crate::renderer) use sector_readiness::{
    prepare_prediction_presentation, prepare_prediction_scene,
};

use super::*;
use crate::messages::ClientText as Text;

impl DesktopHost {
    pub(crate) fn start_remote_attempt(
        &mut self,
        address: &str,
    ) -> crate::client_commands::CommandActionResult {
        match self
            .application
            .start_remote(address.to_owned(), crate::host_time::sample())
        {
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
        match self
            .application
            .start_local(config, crate::host_time::sample())
        {
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

    pub(super) fn connection_attempt_started(&mut self) {
        self.demo_environment = None;
        self.environment_frame = crate::environment::EnvironmentFrameInput::unavailable();
        if self.application.settings.require_earth {
            self.application.status = crate::application::ClientStatus::Status(
                minimium_localization::UiStatus::EarthConnecting,
            );
            self.renderer.scene_visible = false;
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
            StartError::InvalidEndpoint => Text::ConnectFailed("invalid_endpoint".into()),
            StartError::WorkerSpawn(error) => Text::WorkerStartFailed(error.to_string()),
            StartError::Time(error) => Text::WorkerStartFailed(error.to_string()),
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

    pub(super) fn show_operation_result(
        &mut self,
        source: Option<crate::client_commands::CommandSource>,
        result: crate::client_commands::output::ClientResult,
    ) {
        if source == Some(crate::client_commands::CommandSource::DeveloperStdin) {
            super::dev_input::format_output(self, Some(result));
        } else {
            let output = crate::client_commands::output::display(
                Some(result),
                &self.localizer,
                self.application.settings.locale,
            );
            super::controller::format_console_output(self, output);
        }
    }
    pub(super) fn handle_step_connector(
        &mut self,
        admin_result: Option<minimium_commands::admin::AdminReplyV1>,
        connection: ConnectionEvent,
    ) {
        if let Some(reply) = admin_result {
            let (source, result) = self.application.finish_admin(reply);
            self.show_operation_result(source, result);
        }
        match connection {
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
                self.renderer.clear_remote_vertices();
                self.gameplay_presentation.clear_remote_players();
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

    pub(crate) fn finish_on_exit(&mut self, deadline: Instant) -> Result<(), Box<dyn Error>> {
        self.clear_connection();
        let worker_deadline = deadline - Duration::from_secs(2);
        let server_result = self
            .application
            .finish_until(worker_deadline, crate::host_time::sample());
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
        self.cancel_tool_swing();
        if let Err(error) = self.application.validate_time(crate::host_time::sample()) {
            self.application
                .record_diagnostic_failure("client.invalid_time", &error);
            return;
        }
        let step_disconnect = self.pending_step_disconnect.take();
        self.excavation_debug = Default::default();
        self.pending_bolid_request = None;
        self.last_sky_view_at = None;
        self.bolids.clear();
        self.diagnostic_bolid_age = None;
        self.clear_pending_sector_transitions();
        let previous_instance = step_disconnect.as_ref().map_or_else(
            || self.application.core_view().world_instance,
            |disconnect| disconnect.previous_instance,
        );
        let bootstrap = super::sector_scene::SceneInterestOwner::Bootstrap(previous_instance);
        self.sector_cache.release_interest(bootstrap);
        self.renderer
            .release_scene_stage(bootstrap.instance(), bootstrap);
        let previous_context = step_disconnect.as_ref().map_or_else(
            || self.application.core_view().context,
            |disconnect| disconnect.previous_context,
        );
        if let Some(context) = previous_context {
            let active = super::sector_scene::SceneInterestOwner::Active(context);
            self.sector_cache.release_interest(active);
            self.renderer
                .release_scene_stage(context.instance(), active);
        }
        if self.application.settings.require_earth {
            self.renderer.scene_visible = false;
            if !self.application.status.is_error() {
                self.application.status = crate::application::ClientStatus::Status(
                    minimium_localization::UiStatus::EarthDisconnected,
                );
            }
        }
        let session = match step_disconnect {
            Some(disconnect) => disconnect.session,
            None => match self
                .application
                .disconnect_session(crate::host_time::sample())
            {
                Ok(session) => session,
                Err(error) => {
                    self.application
                        .record_diagnostic_failure("client.invalid_time", &error);
                    None
                }
            },
        };
        if let Some(mut session) = session {
            if let Err(error) = session.close(crate::host_time::sample()) {
                self.application
                    .record_diagnostic_failure("client.invalid_time", &error);
            }
        }
        self.renderer.clear_remote_vertices();
        self.gameplay_presentation.clear_remote_players();
    }

    pub(super) fn fail_connection(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.application.status = crate::application::ClientStatus::Message(
            minimium_localization::UiMessage::EarthError(message.clone()),
        );
        if self.application.settings.require_earth {
            self.renderer.scene_visible = false;
        }
        self.application
            .connection_view()
            .record_diagnostic_failure("client_runtime.session_content_failed", &message);
        self.application.ui.console.push_ui_error_message(
            minimium_localization::UiMessage::ContentUnavailable,
            &self.localizer,
            self.application.settings.locale,
        );
        self.clear_connection();
    }

    pub(super) fn handle_step_session_events(
        &mut self,
        events: Vec<crate::application::AppliedSessionEvent>,
        disconnecting: bool,
    ) {
        if self.application.connection_view().session().is_some() {
            self.prepare_session_materials();
        }
        for applied in events {
            let crate::application::AppliedSessionEvent { event, update } = applied;
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
                        self.renderer.scene_visible = false;
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
                    if disconnecting {
                        if let Err(error) = update {
                            self.handle_detached_message_error(error);
                        }
                        if matches!(message, ServerMessage::Bye) && self.application.ui.console.open
                        {
                            self.application.ui.console.push_client_text(
                                Text::ServerClosed,
                                false,
                                &self.localizer,
                                self.application.settings.locale,
                            );
                        }
                        continue;
                    }
                    self.handle_server_message(message, update);
                    if self.application.connection_view().session().is_none() {
                        break;
                    }
                }
            }
        }
        if self
            .application
            .connection_view()
            .session()
            .is_some_and(|session| session.state() == SessionState::Disconnected)
        {
            self.clear_connection();
        }
    }

    fn handle_detached_message_error(
        &mut self,
        error: crate::application::ApplicationMessageError,
    ) {
        use crate::application::ApplicationMessageError as MessageError;
        match error {
            MessageError::Time(error) => self
                .application
                .record_diagnostic_failure("client.invalid_time", &error),
            MessageError::StaleWorldContext => {}
            MessageError::VisiblePlayerLimitExceeded => self.fail_connection(
                Text::VisiblePlayerLimitExceeded
                    .localized(&self.localizer, self.application.settings.locale),
            ),
            #[cfg(feature = "desktop")]
            MessageError::RequiredEarthUnavailable => {
                self.fail_connection("serwer nie udostępnia manifestu Earth")
            }
            MessageError::MissingWorldContent | MessageError::WorldContentMismatch => {
                self.fail_connection(&format!("server world content is not supported: {error:?}"))
            }
        }
    }

    pub(super) fn queue_fire(&mut self) {
        let view = self.application.core_view();
        let character = view.character;
        let feet_m = view.position_mm.map_or_else(
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
        match self.application.execute_action(
            crate::application::ClientAction::UseEquipped {
                direction,
                trigger: crate::application::UseTrigger::Pressed,
            },
            crate::host_time::sample(),
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
            crate::application::LocalActionOutcome::Queued {
                kind: crate::application::QueuedActionKind::ToolUse,
                ..
            } => self.start_tool_swing(),
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
        let position = self.application.core_view().character.position;
        let entity = self
            .application
            .connection_view()
            .session()
            .and_then(|session| session.replicated_entities())
            .and_then(|replicas| crate::client_commands::nearest_portal_entity(replicas, position));
        let Some(entity) = entity else { return };
        let _ = self.application.execute_action(
            crate::application::ClientAction::InteractEntity { entity },
            crate::host_time::sample(),
        );
    }
}

mod remote_players;
mod world_install;
