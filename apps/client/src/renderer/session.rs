mod message_handling;

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

    pub(crate) fn start_local_attempt(&mut self, threads: crate::options::ThreadLane) {
        self.demo_environment = None;
        self.environment_frame = crate::environment::EnvironmentFrameInput::unavailable();
        if self.settings.require_earth {
            self.connection_status = "Łączenie z Earth".into();
            self.gpu.scene_visible = false;
        }
        match self.connection.start_local(
            self.settings.data_dir.clone(),
            self.settings.map_path.clone(),
            self.settings.world_definition_path.clone(),
            threads,
        ) {
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
        self.gpu.remote_vertex_count = 0;
        self.game.remote_player_count = 0;
        self.game.remote_vertices.clear();
        self.game.remote_players = [None; MAX_VISIBLE_PLAYERS];
        self.game.last_remote_snapshot_time_ms = None;
        self.game.last_remote_snapshot_received_at = None;
        self.game.remote_geometry_dirty = false;
        self.game.health = PLAYER_MAX_HEALTH;
        self.game.hit_confirmation_until = None;
        self.game.last_shot_sent_at = None;
    }

    pub(crate) fn finish_on_exit(&mut self) -> Result<(), Box<dyn Error>> {
        self.clear_connection();
        self.connection.finish()?;
        Ok(())
    }

    pub(super) fn clear_connection(&mut self) {
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
        self.game.last_shot_sent_at = None;
        self.gpu.remote_vertex_count = 0;
        self.game.remote_player_count = 0;
        self.game.remote_vertices.clear();
        self.game.remote_players = [None; MAX_VISIBLE_PLAYERS];
        self.game.last_remote_snapshot_time_ms = None;
        self.game.last_remote_snapshot_received_at = None;
        self.game.remote_geometry_dirty = false;
        self.game.health = PLAYER_MAX_HEALTH;
        self.game.hit_confirmation_until = None;
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
            .last_shot_sent_at
            .is_some_and(|last_shot| now.saturating_duration_since(last_shot) < SHOT_LOCAL_COOLDOWN)
        {
            return;
        }
        let direction =
            super::camera::camera_direction(self.game.character, self.input.yaw, self.input.pitch)
                .normalize()
                .to_array();
        if let Some(session) = self.connection.session_mut() {
            match session.queue(&ClientMessage::Fire { direction }) {
                Ok(()) => {
                    self.game.last_shot_sent_at = Some(now);
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
            self.game.clear_remote_players();
            self.gpu.remote_vertex_count = 0;
            self.game.remote_shadow_vertices.clear();
            self.gpu.update_remote_shadow_vertices(&[]);
            return;
        }
        if self
            .game
            .update_remote_vertices(now, self.gpu.render_origin)
        {
            self.gpu.update_remote_vertices(&self.game.remote_vertices);
        }
        let center = self.game.position_mm.map_or_else(
            || self.game.character.position.map(f64::from),
            |position| position.0.map(|value| value as f64 / 1000.0),
        );
        let mut shadow_vertices = Vec::with_capacity(self.game.remote_players.len() * 36);
        for track in self.game.remote_players.iter().flatten() {
            let feet = track
                .displayed_position_mm
                .map(|value| value as f64 / 1000.0);
            let shape_center = [feet[0], feet[1] + 0.85, feet[2]];
            let half = [0.3, 0.85, 0.3];
            let distance_squared = (0..3)
                .map(|axis| {
                    let delta = (center[axis] - shape_center[axis]).abs() - half[axis];
                    delta.max(0.0).powi(2)
                })
                .sum::<f64>();
            if distance_squared > 256.0_f64.powi(2) {
                continue;
            }
            let relative: [f32; 3] = std::array::from_fn(|axis| {
                (feet[axis] - f64::from(self.gpu.render_origin[axis])) as f32
            });
            cube(
                &mut shadow_vertices,
                [relative[0], relative[1] + 0.85, relative[2]],
                [0.6, 1.7, 0.6],
                [0.82, 0.16, 0.12],
            );
        }
        if self.gpu.shadow_remote_dirty
            || bytemuck::cast_slice::<Vertex, u8>(&shadow_vertices)
                != bytemuck::cast_slice::<Vertex, u8>(&self.game.remote_shadow_vertices)
        {
            self.game.remote_shadow_vertices = shadow_vertices;
            self.gpu
                .update_remote_shadow_vertices(&self.game.remote_shadow_vertices);
        }
    }
}

struct SnapshotUpdate {
    server_time_ms: u64,
    full_snapshot: bool,
    self_position: [f32; 3],
    self_position_mm: minimium_core::world::Position3Mm,
    self_velocity: [f32; 3],
    self_grounded: bool,
    players: minimium_protocol::RemotePlayers,
    removed_players: minimium_protocol::RemotePlayerIds,
}

impl GameplayView {
    fn apply_snapshot(&mut self, snapshot: SnapshotUpdate) -> bool {
        let SnapshotUpdate {
            server_time_ms,
            full_snapshot,
            self_position,
            self_position_mm,
            self_velocity,
            self_grounded,
            players,
            removed_players,
        } = snapshot;
        let server_time_ms = self
            .last_remote_snapshot_time_ms
            .map_or(server_time_ms, |previous| previous.max(server_time_ms));
        self.last_remote_snapshot_time_ms = Some(server_time_ms);
        self.last_remote_snapshot_received_at = Some(Instant::now());
        if full_snapshot {
            self.remote_players = [None; MAX_VISIBLE_PLAYERS];
        }
        for id in removed_players.iter() {
            if let Some(slot) = self
                .remote_players
                .iter_mut()
                .find(|slot| slot.is_some_and(|track| track.id == *id))
            {
                *slot = None;
            }
        }
        for player in players.iter() {
            let existing_slot = self
                .remote_players
                .iter()
                .position(|slot| slot.is_some_and(|track| track.id == player.id));
            let slot =
                existing_slot.or_else(|| self.remote_players.iter().position(Option::is_none));
            let Some(slot) = slot else {
                return false;
            };
            let existing = self.remote_players[slot];
            let previous_position_mm = existing
                .map(|track| track.current_position_mm)
                .unwrap_or(player.position_mm.0);
            let displacement_squared = previous_position_mm
                .iter()
                .zip(player.position_mm.0)
                .map(|(previous, current)| (*previous as f64 - current as f64).powi(2))
                .sum::<f64>();
            let teleported = displacement_squared > 100_000_f64.powi(2);
            let previous_position_mm = if teleported {
                player.position_mm.0
            } else {
                previous_position_mm
            };
            let previous_time_ms = existing
                .map(|track| track.current_time_ms.min(server_time_ms))
                .unwrap_or(server_time_ms);
            let previous_time_ms = if teleported {
                server_time_ms
            } else {
                previous_time_ms
            };
            self.remote_players[slot] = Some(RemotePlayerTrack {
                id: player.id,
                previous_position_mm,
                current_position_mm: player.position_mm.0,
                displayed_position_mm: previous_position_mm,
                previous_time_ms,
                current_time_ms: server_time_ms,
                displayed_time_ms: previous_time_ms,
            });
        }
        self.remote_player_count = self.remote_players.iter().flatten().count();
        self.remote_geometry_dirty = true;
        if let Some(predicted_mm) = &mut self.position_mm {
            predicted_mm.0 = std::array::from_fn(|axis| {
                (predicted_mm.0[axis] as f64 * 0.75 + self_position_mm.0[axis] as f64 * 0.25)
                    .round() as i64
            });
            self.character.position = predicted_mm.0.map(|value| value as f32 / 1000.0);
        } else {
            let predicted = glam::Vec3::from_array(self.character.position);
            let authoritative = glam::Vec3::from_array(self_position);
            self.character.position = predicted.lerp(authoritative, 0.25).to_array();
        }
        self.character.velocity = self_velocity;
        self.character.grounded = self_grounded;
        true
    }

    fn clear_remote_players(&mut self) {
        self.remote_players = [None; MAX_VISIBLE_PLAYERS];
        self.last_remote_snapshot_time_ms = None;
        self.last_remote_snapshot_received_at = None;
        self.last_shot_sent_at = None;
        self.remote_player_count = 0;
        self.remote_vertices.clear();
        self.remote_geometry_dirty = false;
    }

    fn update_remote_vertices(&mut self, now: Instant, origin: [f32; 3]) -> bool {
        let estimated_server_time_ms = match (
            self.last_remote_snapshot_time_ms,
            self.last_remote_snapshot_received_at,
        ) {
            (Some(server_time_ms), Some(received_at)) => {
                let elapsed_ms =
                    u64::try_from(now.saturating_duration_since(received_at).as_millis())
                        .unwrap_or(u64::MAX);
                server_time_ms.saturating_add(elapsed_ms)
            }
            _ => 0,
        };
        let render_time_ms =
            estimated_server_time_ms.saturating_sub(REMOTE_INTERPOLATION_DELAY.as_millis() as u64);

        let mut moved = false;
        for track in self.remote_players.iter_mut().flatten() {
            let interval_ms = track.current_time_ms.saturating_sub(track.previous_time_ms);
            let interpolation = if interval_ms == 0 {
                1.0
            } else {
                (render_time_ms.saturating_sub(track.previous_time_ms) as f64 / interval_ms as f64)
                    .clamp(0.0, 1.0)
            };
            let position_mm = std::array::from_fn(|axis| {
                (track.previous_position_mm[axis] as f64 * (1.0 - interpolation)
                    + track.current_position_mm[axis] as f64 * interpolation)
                    .round() as i64
            });
            if position_mm != track.displayed_position_mm {
                track.displayed_position_mm = position_mm;
                moved = true;
            }
            track.displayed_time_ms = render_time_ms.clamp(
                track.previous_time_ms,
                track.current_time_ms.max(track.previous_time_ms),
            );
        }
        if !moved && !self.remote_geometry_dirty {
            return false;
        }

        self.remote_vertices.clear();
        for track in self.remote_players.iter().flatten() {
            let position: [f32; 3] = std::array::from_fn(|axis| {
                (track.displayed_position_mm[axis] as f64 / 1000.0 - f64::from(origin[axis])) as f32
            });
            cube(
                &mut self.remote_vertices,
                [position[0], position[1] + 0.85, position[2]],
                [0.6, 1.7, 0.6],
                [0.82, 0.16, 0.12],
            );
        }
        self.remote_geometry_dirty = false;
        true
    }
}
