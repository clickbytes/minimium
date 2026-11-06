use super::*;

impl Renderer {
    pub(super) fn handle_altitude_result(
        &mut self,
        context: minimium_protocol::WorldContext,
        position_mm: Option<minimium_world::world::Position3Mm>,
    ) {
        if Some(context) != self.game.session_context {
            return;
        }
        if let Some(position) = position_mm {
            self.apply_altitude_position(position);
            self.ui
                .console
                .push_line("set_altitude: destination reached");
        } else {
            self.ui
                .console
                .push_error("set_altitude: rejected; requires fly on, a planet and a valid height");
        }
    }

    pub(super) fn handle_map_teleport_result(
        &mut self,
        position_mm: Option<minimium_world::world::Position3Mm>,
    ) {
        self.map_teleport_pending = false;
        if let Some(position_mm) = position_mm {
            self.game.gameplay.prediction.position_mm = Some(position_mm);
            self.game.gameplay.prediction.character.position =
                position_mm.0.map(|coordinate| coordinate as f32 / 1000.0);
            let origin = self
                .game
                .gameplay
                .prediction
                .character
                .position
                .map(|coordinate| (coordinate / 256.0).round() * 256.0);
            self.gpu.set_render_origin(origin);
            self.game.gameplay.prediction.character.velocity = [0.0; 3];
            self.game.gameplay.prediction.character.grounded = false;
            self.game.gameplay.prediction.reset_time();
            self.game.gameplay.sectors_ready = false;
            self.teleport_loading = true;
            self.gpu.scene_visible = false;
            self.last_render_position = [f32::INFINITY; 3];
            self.input.keys.clear();
            self.input.last_sent_direction = [0.0; 2];
            self.connection_status = "Earth: wczytywanie terenu...".into();
            self.ui.console.push_line("map_telepo: destination reached");
        } else {
            self.connection_status = "Earth: wybierz punkt na lądzie".into();
            self.ui
                .console
                .push_error("map_telepo: destination unavailable; choose land");
            if !self.overview_open && self.gpu.scene_visible {
                self.toggle_overview();
                self.map_teleport_armed = true;
            }
        }
    }

    pub(super) fn handle_shot_result(
        &mut self,
        target_id: Option<minimium_core::ids::PlayerId>,
        remaining_health: Option<u16>,
        killed: bool,
    ) {
        if target_id.is_some() {
            self.game.gameplay.hit_confirmation_until =
                Some(Instant::now() + Duration::from_millis(180));
        }
        if self.ui.console.open {
            match (target_id, remaining_health, killed) {
                (Some(target_id), Some(health), true) => self
                    .ui
                    .console
                    .push_line(Text::TargetRespawned { target_id, health }.to_string()),
                (Some(target_id), Some(health), false) => self
                    .ui
                    .console
                    .push_line(Text::Hit { target_id, health }.to_string()),
                _ => self.ui.console.push_line(Text::ShotMissed.to_string()),
            }
        }
    }

    pub(super) fn handle_health_update(&mut self, health: u16) {
        self.game.gameplay.health = health;
        if self.ui.console.open {
            self.ui
                .console
                .push_line(Text::Health { health }.to_string());
        }
    }

    pub(super) fn handle_snapshot(
        &mut self,
        server_time_ms: u64,
        full_snapshot: bool,
        self_position: [f32; 3],
        self_position_mm: minimium_world::world::Position3Mm,
        self_velocity: [f32; 3],
        self_grounded: bool,
        players: Box<minimium_protocol::RemotePlayers>,
        removed_players: minimium_protocol::RemotePlayerIds,
    ) {
        if !self.game.gameplay.apply_snapshot(
            server_time_ms,
            full_snapshot,
            self_position,
            self_position_mm,
            self_velocity,
            self_grounded,
            &players,
            &removed_players,
        ) {
            eprintln!("{}", Text::VisiblePlayerLimitExceeded);
            self.fail_connection(Text::VisiblePlayerLimitExceeded.to_string());
        }
    }
}
