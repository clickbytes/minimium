mod message_handling;
mod sector_readiness;

use super::*;
use crate::messages::ClientText as Text;

impl Renderer {
    pub(crate) fn start_remote_attempt(&mut self, address: &str) {
        self.demo_environment = None;
        self.environment_frame = crate::environment::EnvironmentFrameInput::unavailable();
        if self.settings.require_earth {
            self.connection_status = "Łączenie z Earth".into();
            self.gpu.scene_visible = false;
        }
        match self.connection.start_remote(address.to_owned()) {
            Ok(()) => self
                .ui
                .console
                .push_line(Text::ConnectingTo(address.to_owned()).to_string()),
            Err(error) => self.report_start_error(error),
        }
    }

    pub(crate) fn start_local_attempt(&mut self, threads: Option<crate::options::ThreadLane>) {
        self.demo_environment = None;
        self.environment_frame = crate::environment::EnvironmentFrameInput::unavailable();
        if self.settings.require_earth {
            self.connection_status = "Łączenie z Earth".into();
            self.gpu.scene_visible = false;
        }
        let mut config = self.settings.local_server_config.clone();
        if let Some(threads) = threads {
            if config
                .settings
                .insert("server.threads".to_owned(), threads.as_str().to_owned())
                .is_some()
            {
                self.ui.console.push_error(
                    "server.threads was specified both in client options and the host command",
                );
                return;
            }
        }
        match self.connection.start_local(config) {
            Ok(()) => self
                .ui
                .console
                .push_line(Text::StartingLocalServer.to_string()),
            Err(error) => self.report_start_error(error),
        }
    }

    fn report_start_error(&mut self, error: StartError) {
        let message = match error {
            StartError::Busy { active_attempt } => Text::BusyAttempt {
                attempt_id: active_attempt.get(),
            }
            .to_string(),
            StartError::AttemptIdsExhausted => Text::AttemptIdsExhausted.to_string(),
            StartError::WorkerSpawn(error) => {
                Text::WorkerStartFailed(error.to_string()).to_string()
            }
        };
        if self.settings.require_earth {
            self.connection_status = format!("Błąd Earth: {message}");
        }
        self.ui.console.push_error(message);
    }

    pub(super) fn poll_connector(&mut self) {
        if let Some(result) = self.connection.poll_time_scale_result() {
            match result {
                Ok(response) => self.ui.console.push_line(response.trim()),
                Err(error) => self.ui.console.push_error(format!("time scale: {error}")),
            }
        }
        match self.connection.poll_connector() {
            ConnectionEvent::TimedOut => {
                self.connection_status = "Błąd połączenia: przekroczono czas".into();
                self.ui
                    .console
                    .push_error(Text::AttemptTimedOut.to_string());
            }
            ConnectionEvent::SessionStarted => {
                self.clear_pending_sector_transitions();
                self.reset_remote_gameplay();
                self.ui.console.push_line(Text::AwaitingWelcome.to_string());
            }
            ConnectionEvent::SetupFailed(error) => {
                self.connection_status = format!("Błąd Earth: {error}");
                self.ui
                    .console
                    .push_error(Text::ConnectionSetupFailed(error.to_string()).to_string());
            }
            ConnectionEvent::Failed(error) => {
                self.connection_status = format!("Błąd Earth: {error}");
                self.ui
                    .console
                    .push_error(Text::ConnectFailed(error.to_string()).to_string());
            }
            _ => {}
        }
    }

    fn reset_remote_gameplay(&mut self) {
        self.gpu.remote_vertices.clear();
        self.game.gameplay.remote_vertices.clear();
        self.game.gameplay.reconciliation.clear();
        self.game.gameplay.remote_geometry_dirty = false;
        self.game.gameplay.health = PLAYER_MAX_HEALTH;
        self.game.gameplay.hit_confirmation_until = None;
        self.game.gameplay.last_shot_sent_at = None;
    }

    pub(crate) fn finish_on_exit(&mut self) -> Result<(), Box<dyn Error>> {
        self.clear_connection();
        self.connection.finish()?;
        Ok(())
    }

    pub(super) fn clear_connection(&mut self) {
        self.game.session_context = None;
        self.clear_pending_sector_transitions();
        self.map_teleport_pending = false;
        self.map_teleport_armed = false;
        self.teleport_loading = false;
        if self.settings.require_earth {
            self.gpu.scene_visible = false;
            if !self.connection_status.starts_with("Błąd") {
                self.connection_status = "Earth: rozłączono".into();
            }
        }
        if let Some(mut session) = self.connection.take_session() {
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
        self.game.gameplay.last_shot_sent_at = None;
        self.gpu.remote_vertices.clear();
        self.game.gameplay.remote_vertices.clear();
        self.game.gameplay.reconciliation.clear();
        self.game.gameplay.remote_geometry_dirty = false;
        self.game.gameplay.health = PLAYER_MAX_HEALTH;
        self.game.gameplay.hit_confirmation_until = None;
    }

    pub(super) fn fail_connection(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.connection_status = format!("Błąd Earth: {message}");
        if self.settings.require_earth {
            self.gpu.scene_visible = false;
        }
        eprintln!("{message}");
        self.ui.console.push_error(message);
        self.clear_connection();
    }

    pub(super) fn poll_session(&mut self) {
        if self.connection.session().is_some() {
            self.ui.session_events.clear();
            self.connection.poll_session(&mut self.ui.session_events);
            self.prepare_session_materials();
        }
        let mut events = std::mem::take(&mut self.ui.session_events);
        for event in events.drain(..) {
            match event {
                SessionEvent::PingRoundTrip(duration) => {
                    if self.ui.console.open {
                        self.ui.console.push_line(
                            Text::Ping {
                                milliseconds: duration.as_millis(),
                            }
                            .to_string(),
                        );
                    }
                }
                SessionEvent::Disconnected(reason) => {
                    if reason == DisconnectReason::ServerClosed {
                        if self.settings.require_earth {
                            self.connection_status = "Błąd Earth: serwer zamknął połączenie".into();
                        }
                        if self.ui.console.open {
                            self.ui.console.push_line(Text::ServerClosed.to_string());
                        }
                        self.clear_connection();
                        continue;
                    }
                    let message = Text::from(reason).to_string();
                    self.fail_connection(message);
                }
                SessionEvent::Message(message) => {
                    self.handle_server_message(message);
                    if self.connection.session().is_none() {
                        break;
                    }
                }
            }
        }
        if self
            .connection
            .session()
            .is_some_and(|session| session.state() == SessionState::Disconnected)
        {
            self.clear_connection();
        }
        self.ui.session_events = events;
    }

    pub(super) fn queue_fire(&mut self) {
        if !self
            .connection
            .session()
            .is_some_and(|session| session.state() == SessionState::Connected)
        {
            return;
        }
        let now = Instant::now();
        if self
            .game
            .gameplay
            .last_shot_sent_at
            .is_some_and(|last_shot| now.saturating_duration_since(last_shot) < SHOT_LOCAL_COOLDOWN)
        {
            return;
        }
        let direction = super::camera::camera_direction(
            self.game.gameplay.prediction.character,
            self.input.yaw,
            self.input.pitch,
        )
        .normalize()
        .to_array();
        if let Some(session) = self.connection.session_mut() {
            match session.queue(&ClientMessage::Fire { direction }) {
                Ok(()) => {
                    self.game.gameplay.last_shot_sent_at = Some(now);
                    if self.ui.console.open {
                        self.ui.console.push_line(Text::ShotFired.to_string());
                    }
                }
                Err(error) if self.ui.console.open => self
                    .ui
                    .console
                    .push_error(Text::CouldNotFire(error.into()).to_string()),
                Err(_) => {}
            }
        }
    }

    pub(crate) fn queue_portal(&mut self) {
        let Some(session) = self.connection.session_mut() else {
            return;
        };
        if session.state() == SessionState::Connected {
            let _ = session.queue(&ClientMessage::UsePortal);
        }
    }

    pub(super) fn update_remote_player_rendering(&mut self, now: Instant) {
        if self.connection.session().is_none() {
            self.game.gameplay.clear_remote_players();
            self.gpu.remote_vertices.clear();
            self.gpu.update_remote_shadow_vertices(&[]);
            return;
        }
        if self
            .game
            .gameplay
            .update_remote_vertices(now, self.gpu.render_origin)
        {
            self.gpu
                .update_remote_vertices(&self.game.gameplay.remote_vertices);
        }
        let center = self.game.gameplay.prediction.position_mm.map_or_else(
            || {
                self.game
                    .gameplay
                    .prediction
                    .character
                    .position
                    .map(f64::from)
            },
            |position| position.0.map(|value| value as f64 / 1000.0),
        );
        let shadow_vertices_changed = self
            .game
            .gameplay
            .update_remote_shadow_vertices(center, self.gpu.render_origin);
        if self.gpu.shadow_remote_dirty || shadow_vertices_changed {
            self.gpu
                .update_remote_shadow_vertices(&self.game.gameplay.remote_shadow_vertices);
        }
    }
}

mod world_install;
