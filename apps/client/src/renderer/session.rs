use super::*;
use crate::messages::ClientText as Text;

impl Renderer {
    pub(crate) fn start_remote_attempt(&mut self, address: &str) {
        match self
            .connector
            .start(address.to_owned(), CONNECT_ATTEMPT_TIMEOUT)
        {
            Ok(id) => {
                self.active_attempt = Some(id);
                self.console
                    .push_line(Text::ConnectingTo(address.to_owned()).to_string());
            }
            Err(error) => self.report_start_error(error),
        }
    }

    pub(crate) fn start_local_attempt(&mut self, threads: crate::options::ThreadLane) {
        match self.connector.start_local(
            self.data_dir.clone(),
            self.map_path.clone(),
            threads,
            CONNECT_ATTEMPT_TIMEOUT,
        ) {
            Ok(id) => {
                self.active_attempt = Some(id);
                self.console
                    .push_line(Text::StartingLocalServer.to_string());
            }
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
        self.console.push_error(message);
    }

    pub(super) fn poll_connector(&mut self) {
        match self.connector.poll() {
            PollEvent::Idle => self.active_attempt = None,
            PollEvent::InProgress { attempt_id, .. } => self.active_attempt = Some(attempt_id),
            PollEvent::TimedOut { attempt_id } if self.active_attempt == Some(attempt_id) => {
                self.console.push_error(Text::AttemptTimedOut.to_string());
            }
            PollEvent::Cancelled { attempt_id } if self.active_attempt == Some(attempt_id) => {
                self.active_attempt = None;
            }
            PollEvent::Connected {
                attempt_id,
                stream,
                local_server,
            } if self.active_attempt == Some(attempt_id) => {
                self.active_attempt = None;
                if let Some(server) = local_server {
                    crate::retain_local_server(&mut self.local_server, server);
                }
                match ClientSession::new(stream, self.profile_id, self.map_fingerprint) {
                    Ok(session) => {
                        self.session = Some(session);
                        self.reset_remote_gameplay();
                        self.console.push_line(Text::AwaitingWelcome.to_string());
                    }
                    Err(error) => self
                        .console
                        .push_error(Text::ConnectionSetupFailed(error.to_string()).to_string()),
                }
            }
            PollEvent::Failed { attempt_id, error } if self.active_attempt == Some(attempt_id) => {
                self.active_attempt = None;
                self.console
                    .push_error(Text::ConnectFailed(error.to_string()).to_string());
            }
            _ => {}
        }
    }

    fn reset_remote_gameplay(&mut self) {
        self.remote_vertex_count = 0;
        self.remote_player_count = 0;
        self.remote_vertices.clear();
        self.remote_players = [None; MAX_VISIBLE_PLAYERS];
        self.last_remote_snapshot_time_ms = None;
        self.last_remote_snapshot_received_at = None;
        self.remote_geometry_dirty = false;
        self.health = PLAYER_MAX_HEALTH;
        self.hit_confirmation_until = None;
        self.last_shot_sent_at = None;
    }

    pub(crate) fn finish_on_exit(&mut self) -> Result<(), Box<dyn Error>> {
        if let Some(id) = self.active_attempt.take() {
            self.connector.cancel(id);
        }
        self.connector.finish_on_exit();
        self.clear_connection();
        if let Some(mut server) = self.local_server.take() {
            server.shutdown()?;
        }
        Ok(())
    }

    pub(super) fn clear_connection(&mut self) {
        if let Some(mut session) = self.session.take() {
            let metrics = session.metrics();
            self.previous_bytes_sent = self.previous_bytes_sent.saturating_add(metrics.bytes_sent);
            self.previous_bytes_received = self
                .previous_bytes_received
                .saturating_add(metrics.bytes_received);
            session.close();
        }
        self.last_shot_sent_at = None;
        self.remote_vertex_count = 0;
        self.remote_player_count = 0;
        self.remote_vertices.clear();
        self.remote_players = [None; MAX_VISIBLE_PLAYERS];
        self.last_remote_snapshot_time_ms = None;
        self.last_remote_snapshot_received_at = None;
        self.remote_geometry_dirty = false;
        self.health = PLAYER_MAX_HEALTH;
        self.hit_confirmation_until = None;
    }

    pub(super) fn fail_connection(&mut self, message: impl Into<String>) {
        let message = message.into();
        eprintln!("{message}");
        self.console.push_error(message);
        self.clear_connection();
    }

    pub(super) fn poll_session(&mut self) {
        if let Some(session) = self.session.as_mut() {
            self.session_events.clear();
            session.poll(&mut self.session_events);
        }
        let mut events = std::mem::take(&mut self.session_events);
        for event in events.drain(..) {
            match event {
                SessionEvent::PingRoundTrip(duration) => {
                    if self.console.open {
                        self.console.push_line(
                            Text::Ping {
                                milliseconds: duration.as_millis(),
                            }
                            .to_string(),
                        );
                    }
                }
                SessionEvent::Disconnected(reason) => {
                    if reason == DisconnectReason::ServerClosed {
                        if self.console.open {
                            self.console.push_line(Text::ServerClosed.to_string());
                        }
                        self.clear_connection();
                        continue;
                    }
                    let message = Text::from(reason).to_string();
                    self.fail_connection(message);
                }
                SessionEvent::Message(message) => {
                    self.handle_server_message(message);
                    if self.session.is_none() {
                        break;
                    }
                }
            }
        }
        if self
            .session
            .as_ref()
            .is_some_and(|session| session.state() == SessionState::Disconnected)
        {
            self.clear_connection();
        }
        self.session_events = events;
    }

    fn handle_server_message(&mut self, message: ServerMessage) {
        match message {
            ServerMessage::Welcome {
                id,
                map_fingerprint: _,
                health,
            } => {
                self.health = health;
                if self.console.open {
                    self.console
                        .push_line(Text::Connected { id, health }.to_string());
                }
            }
            ServerMessage::Pong => {
                if self.console.open {
                    self.console.push_line(Text::Pong.to_string());
                }
            }
            ServerMessage::Accepted if self.console.open => {
                self.console.push_line(Text::Accepted.to_string());
            }
            ServerMessage::Error(code) => {
                self.console
                    .push_error(Text::ServerError { code }.to_string());
            }
            ServerMessage::ShotResult {
                target_id,
                remaining_health,
                killed,
            } => {
                if target_id.is_some() {
                    self.hit_confirmation_until = Some(Instant::now() + Duration::from_millis(180));
                }
                if self.console.open {
                    match (target_id, remaining_health, killed) {
                        (Some(target_id), Some(health), true) => self
                            .console
                            .push_line(Text::TargetRespawned { target_id, health }.to_string()),
                        (Some(target_id), Some(health), false) => self
                            .console
                            .push_line(Text::Hit { target_id, health }.to_string()),
                        _ => self.console.push_line(Text::ShotMissed.to_string()),
                    }
                }
            }
            ServerMessage::HealthUpdate(health) => {
                self.health = health;
                if self.console.open {
                    self.console.push_line(Text::Health { health }.to_string());
                }
            }
            ServerMessage::Chat {
                sender_id,
                sender_name,
                text,
            } => self.console.push_chat_line(
                &Text::Chat {
                    sender_name,
                    sender_id,
                    text,
                }
                .to_string(),
            ),
            ServerMessage::Bye => {
                if self.console.open {
                    self.console.push_line(Text::ServerClosed.to_string());
                }
                self.clear_connection();
            }
            ServerMessage::Snapshot {
                server_time_ms,
                full_snapshot,
                self_position,
                self_velocity,
                self_grounded,
                players,
                removed_players,
            } => {
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
                    let slot = existing_slot
                        .or_else(|| self.remote_players.iter().position(Option::is_none));
                    let Some(slot) = slot else {
                        eprintln!("{}", Text::VisiblePlayerLimitExceeded);
                        self.fail_connection(Text::VisiblePlayerLimitExceeded.to_string());
                        return;
                    };
                    let existing = self.remote_players[slot];
                    let previous_position = existing
                        .map(|track| track.current_position)
                        .unwrap_or(player.position);
                    let previous_time_ms = existing
                        .map(|track| track.current_time_ms.min(server_time_ms))
                        .unwrap_or(server_time_ms);
                    self.remote_players[slot] = Some(RemotePlayerTrack {
                        id: player.id,
                        previous_position,
                        current_position: player.position,
                        displayed_position: previous_position,
                        previous_time_ms,
                        current_time_ms: server_time_ms,
                        displayed_time_ms: previous_time_ms,
                    });
                }
                self.remote_player_count = self.remote_players.iter().flatten().count();
                self.remote_geometry_dirty = true;
                let predicted = glam::Vec3::from_array(self.character.position);
                let authoritative = glam::Vec3::from_array(self_position);
                self.character.position = predicted.lerp(authoritative, 0.25).to_array();
                self.character.velocity = self_velocity;
                self.character.grounded = self_grounded;
            }
            _ => {}
        }
    }

    pub(super) fn queue_fire(&mut self) {
        if !self
            .session
            .as_ref()
            .is_some_and(|session| session.state() == SessionState::Connected)
        {
            return;
        }
        let now = Instant::now();
        if self
            .last_shot_sent_at
            .is_some_and(|last_shot| now.saturating_duration_since(last_shot) < SHOT_LOCAL_COOLDOWN)
        {
            return;
        }
        let direction = glam::Vec3::new(
            self.yaw.cos() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.sin() * self.pitch.cos(),
        )
        .normalize()
        .to_array();
        if let Some(session) = self.session.as_mut() {
            match session.queue(&ClientMessage::Fire { direction }) {
                Ok(()) => {
                    self.last_shot_sent_at = Some(now);
                    if self.console.open {
                        self.console.push_line(Text::ShotFired.to_string());
                    }
                }
                Err(error) if self.console.open => self
                    .console
                    .push_error(Text::CouldNotFire(error.into()).to_string()),
                Err(_) => {}
            }
        }
    }

    pub(super) fn update_remote_player_rendering(&mut self, now: Instant) {
        if self.session.is_none() {
            self.remote_players = [None; MAX_VISIBLE_PLAYERS];
            self.last_remote_snapshot_time_ms = None;
            self.last_remote_snapshot_received_at = None;
            self.last_shot_sent_at = None;
            self.remote_player_count = 0;
            self.remote_vertex_count = 0;
            self.remote_vertices.clear();
            self.remote_geometry_dirty = false;
            return;
        }

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
                    .clamp(0.0, 1.0) as f32
            };
            let position = glam::Vec3::from_array(track.previous_position)
                .lerp(
                    glam::Vec3::from_array(track.current_position),
                    interpolation,
                )
                .to_array();
            if position != track.displayed_position {
                track.displayed_position = position;
                moved = true;
            }
            track.displayed_time_ms = render_time_ms.clamp(
                track.previous_time_ms,
                track.current_time_ms.max(track.previous_time_ms),
            );
        }
        if !moved && !self.remote_geometry_dirty {
            return;
        }

        self.remote_vertices.clear();
        for track in self.remote_players.iter().flatten() {
            let position = track.displayed_position;
            cube(
                &mut self.remote_vertices,
                [position[0], position[1] + 0.85, position[2]],
                [0.6, 1.7, 0.6],
                [0.82, 0.16, 0.12],
            );
        }
        self.remote_vertex_count = self.remote_vertices.len() as u32;
        if self.remote_vertex_count > 0 {
            self.queue.write_buffer(
                &self.player_buffer,
                0,
                bytemuck::cast_slice(&self.remote_vertices),
            );
        }
        self.remote_geometry_dirty = false;
    }
}
